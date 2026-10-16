//! The router: every route registered through utoipa's `OpenApiRouter` and `routes!`, so the
//! `OpenAPI` document is generated from the same handlers axum serves; groups that differ only in
//! their timeout and body limit; and the global layer stack.

use std::time::Duration;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request},
    http::{HeaderName, HeaderValue, Method, StatusCode, header},
    middleware,
    routing::get,
};
use tower::ServiceBuilder;
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::{
        CompressionLayer,
        predicate::{DefaultPredicate, NotForContentType, Predicate},
    },
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    error::{panic_response, render_errors},
    openapi::ApiDoc,
    routes::{
        account, chat, gardens, internal, media, memories, ops, plants, scans, species, tasks,
    },
    state::SharedState,
    uploads::UPLOAD_BODY_LIMIT,
};

/// The claim long-poll waits up to 30 s ([contracts 5]); its group's timeout leaves 5 s more.
pub const CLAIM_TIMEOUT: Duration = Duration::from_secs(35);
/// Uploads run the CPU pipeline; worker completions carry large results.
pub const SLOW_TIMEOUT: Duration = Duration::from_secs(60);
/// Worker result bodies (perception masks and embeddings for every image).
pub const WORKER_BODY_LIMIT: usize = 16 * 1024 * 1024;
/// JSON bodies of the app and claim routes. Each group sets its own limit, since a group without
/// one gets axum's 2 MiB default.
pub const JSON_BODY_LIMIT: usize = 1024 * 1024;

/// `req_<ULID>` request ids ([contracts 2], `request_id`).
#[derive(Debug, Clone, Copy)]
pub struct MakeReqId;

impl MakeRequestId for MakeReqId {
    fn make_request_id<B>(&mut self, _: &axum::http::Request<B>) -> Option<RequestId> {
        HeaderValue::try_from(format!("req_{}", ulid::Ulid::generate()))
            .ok()
            .map(RequestId::new)
    }
}

/// CORS for the web build ([frontend 17]): the configured origins only, exactly the four request
/// headers, and `x-request-id` and `retry-after` readable by the app.
pub fn cors(origins: &[String]) -> CorsLayer {
    let origins: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("idempotency-key"),
            HeaderName::from_static("last-event-id"),
        ])
        .expose_headers([HeaderName::from_static("x-request-id"), header::RETRY_AFTER])
        .max_age(Duration::from_secs(600))
}

fn timeout(after: Duration) -> TimeoutLayer {
    // 503, rendered as `upstream_unavailable` with `upstream: "api"`, `retryable: true` and
    // `retry_after_s` (contracts 2, timeouts): a request that runs this long is almost
    // always waiting on a slow dependency, and every create call is idempotent, so the app's
    // retry is safe. The handler's future is dropped, which releases its idempotency key.
    TimeoutLayer::with_status_code(StatusCode::SERVICE_UNAVAILABLE, after)
}

/// Every route, grouped. A group's `.layer` wraps only the routes registered in it, which is why
/// the upload and worker routes are built as their own `OpenApiRouter`s and merged.
pub fn routes(request_timeout: Duration) -> OpenApiRouter<SharedState> {
    let app = OpenApiRouter::new()
        .routes(routes!(scans::create_scan))
        .routes(routes!(scans::get_scan, scans::delete_scan))
        .routes(routes!(scans::delete_image, media::scan_image))
        .routes(routes!(media::scan_mask))
        .routes(routes!(scans::submit))
        .routes(routes!(scans::answer))
        .routes(routes!(scans::feedback))
        .routes(routes!(scans::events))
        .routes(routes!(gardens::list_gardens, gardens::create_garden))
        .routes(routes!(
            gardens::get_garden,
            gardens::update_garden,
            gardens::delete_garden
        ))
        .routes(routes!(gardens::garden_weather))
        .routes(routes!(gardens::list_members))
        .routes(routes!(gardens::update_member, gardens::remove_member))
        .routes(routes!(gardens::list_invites, gardens::create_invite))
        .routes(routes!(gardens::revoke_invite))
        .routes(routes!(gardens::accept_invite))
        .routes(routes!(gardens::get_map, gardens::put_map))
        .routes(routes!(plants::list_plants, plants::create_plant))
        .routes(routes!(
            plants::get_plant,
            plants::update_plant,
            plants::delete_plant
        ))
        .routes(routes!(plants::timeline))
        .routes(routes!(plants::measurements))
        .routes(routes!(plants::confirm_species))
        .routes(routes!(species::list_species))
        .routes(routes!(species::get_species))
        .routes(routes!(media::species_photo))
        .routes(routes!(tasks::list_tasks, tasks::create_task))
        .routes(routes!(tasks::update_task, tasks::delete_task))
        .routes(routes!(
            account::get_me,
            account::update_me,
            account::delete_me
        ))
        .routes(routes!(account::register_push_token))
        .routes(routes!(account::remove_push_token))
        .routes(routes!(memories::list_memories, memories::create_memory))
        .routes(routes!(memories::share_memory))
        .routes(routes!(memories::delete_memory))
        .routes(routes!(chat::create_conversation, chat::list_conversations))
        .routes(routes!(chat::get_conversation))
        .routes(routes!(media::chat_image))
        .routes(routes!(chat::send_message))
        .routes(routes!(chat::message_feedback))
        .routes(routes!(chat::confirm_pending_action))
        .routes(routes!(chat::reject_pending_action))
        .routes(routes!(ops::healthz))
        .routes(routes!(ops::status))
        .routes(routes!(ops::auth_webhook))
        .layer(DefaultBodyLimit::max(JSON_BODY_LIMIT))
        .layer(timeout(request_timeout));
    let uploads = OpenApiRouter::new()
        .routes(routes!(scans::upload_image))
        .routes(routes!(chat::upload_chat_image))
        .layer(DefaultBodyLimit::max(UPLOAD_BODY_LIMIT))
        .layer(timeout(SLOW_TIMEOUT));
    let claim = OpenApiRouter::new()
        .routes(routes!(internal::claim))
        .layer(DefaultBodyLimit::max(JSON_BODY_LIMIT))
        .layer(timeout(CLAIM_TIMEOUT));
    let worker = OpenApiRouter::new()
        .routes(routes!(internal::heartbeat))
        .routes(routes!(internal::job_events))
        .routes(routes!(internal::complete))
        .routes(routes!(internal::job_image))
        .layer(DefaultBodyLimit::max(WORKER_BODY_LIMIT))
        .layer(timeout(SLOW_TIMEOUT));
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(app)
        .merge(uploads)
        .merge(claim)
        .merge(worker)
}

/// The `OpenAPI` document (`api openapi` writes it to `backend/openapi.json`).
pub fn openapi() -> utoipa::openapi::OpenApi {
    routes(Duration::from_secs(10)).split_for_parts().1
}

/// The whole application: the routes, `/openapi.json`, the fallback and the global stack, in
/// request order (`ServiceBuilder` applies top to bottom, the first layer seeing the request first):
///
/// | Layer | Why there |
/// |---|---|
/// | drop a client's `x-request-id`, set `req_<ULID>`, propagate it | first, so the span, every layer and every error body see one id, and every response carries it |
/// | `TraceLayer` | outside everything that answers, so the span records the final status |
/// | `CorsLayer` | answers preflights (which carry no `Authorization`) before anything else runs |
/// | `CompressionLayer` | after the error renderer has written its body; skips `text/event-stream` (frames would sit in the encoder) and images |
/// | `render_errors` | writes every error body; everything below it, tower-http's own responses included, gets an `ErrorBody` |
/// | `CatchPanicLayer` | inside the renderer, so a panic's `internal` body gets its `request_id` |
/// | `RequestBodyLimitLayer` | the global byte cap; each group's `DefaultBodyLimit` is no larger and answers with its own number |
pub fn app(state: SharedState) -> Router {
    let cors = cors(&state.config.web_origins);
    let (router, api) = routes(state.config.request_timeout).split_for_parts();
    let spec = Json(api);
    let compression = CompressionLayer::new().compress_when(
        DefaultPredicate::new().and(NotForContentType::const_new("text/event-stream")),
    );
    router
        .route("/openapi.json", get(move || async move { spec.clone() }))
        .fallback(|| async { crate::error::ApiError::not_found() })
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .map_request(|mut request: Request| {
                    request.headers_mut().remove("x-request-id");
                    request
                })
                .layer(SetRequestIdLayer::x_request_id(MakeReqId))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(|request: &Request| {
                            let request_id = request
                                .extensions()
                                .get::<RequestId>()
                                .and_then(|id| id.header_value().to_str().ok())
                                .unwrap_or_default();
                            // The matched route, never the raw path: ids and push tokens stay out of logs.
                            let route = request
                                .extensions()
                                .get::<axum::extract::MatchedPath>()
                                .map_or("unmatched", axum::extract::MatchedPath::as_str);
                            tracing::info_span!("request", %request_id, method = %request.method(), route)
                        })
                        .on_response(DefaultOnResponse::new().level(Level::INFO)),
                )
                .layer(cors)
                .layer(compression)
                .layer(middleware::from_fn(render_errors))
                .layer(CatchPanicLayer::custom(panic_response))
                .layer(RequestBodyLimitLayer::new(WORKER_BODY_LIMIT.max(UPLOAD_BODY_LIMIT))),
        )
}
