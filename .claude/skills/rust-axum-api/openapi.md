# OpenAPI: utoipa 6 and utoipa-axum 0.3

Part of [rust-axum-api](SKILL.md). The spec is generated from the routes and the `domain` types, committed as `backend/openapi.json`, and read by `openapi-typescript` to make the app's types; CI regenerates it and fails on a difference ([frontend 9, generated types](../../../docs/frontend/README.md#generated-types); [architecture 9](../../../docs/architecture/README.md#9-api-surface)). How the `domain` types derive `ToSchema` (optional vs nullable, enums, IDs, timestamps) is [rust-contract-types](../rust-contract-types/SKILL.md)'s; this file is the wiring.

## Rules

- **Register routes through `OpenApiRouter`**, never `axum::Router::route`, for every public endpoint: only handlers collected with [`routes!`](https://docs.rs/utoipa-axum/0.3.0/utoipa_axum/macro.routes.html) reach the spec. [`OpenApiRouter::split_for_parts`](https://docs.rs/utoipa-axum/0.3.0/utoipa_axum/router/struct.OpenApiRouter.html#method.split_for_parts) returns the `axum::Router` and the `OpenApi` built from the same calls, so the spec can't drift from the routes.
- **One `routes!` per path.** `routes!(a, b)` builds one method router from both handlers and registers it on every path they declare (`OpenApiRouter::routes` folds it over all collected paths, `router.rs`), so only group handlers that share a path (`get` and `delete` of `/scans/{id}`). Two handlers with the same method on one path panic when the router is built ([`routes!`](https://docs.rs/utoipa-axum/0.3.0/utoipa_axum/macro.routes.html), "Panics").
- **`#[utoipa::path]` on each handler**, with the method, `path` in axum 0.8's `{id}` syntax (the same string axum routes on), `params`, `request_body` and every response, **including the error statuses** with `body = ErrorBody`, so the app's generated client knows the error shape ([`utoipa::path`](https://docs.rs/utoipa/6.0.0/utoipa/attr.path.html)).
- **SSE endpoints** declare `content_type = "text/event-stream"`. utoipa 6 added OpenAPI 3.2's `item_schema` for event streams, but it only appears in 3.2 output; the spec stays at the default **3.1.0** ([`OpenApiVersion`](https://docs.rs/utoipa/6.0.0/utoipa/openapi/enum.OpenApiVersion.html)) unless the app's codegen is confirmed to read 3.2. The event types are documented by contracts 4, not the spec.
- **Declare the bearer scheme once**: a `Modify` adds an HTTP bearer `SecurityScheme` (`bearer_format("JWT")`) named `clerk`, and `#[openapi(security(("clerk" = [])))]` requires it on every operation, because every app route takes a Clerk session token ([contracts 1, authentication headers](../../../docs/architecture/contracts.md#authentication-headers); [`Modify`](https://docs.rs/utoipa/6.0.0/utoipa/trait.Modify.html), [`HttpBuilder`](https://docs.rs/utoipa/6.0.0/utoipa/openapi/security/struct.HttpBuilder.html)). Signed image URLs take no token and stay out of the spec with the other non-app routes.
- **Internal, webhook and ops routes** stay out of the spec (plain `Router::route` on their own group): the app never calls them.
- **Writing the file**: a subcommand of the `api` binary prints the pretty JSON; CI runs `cargo run -p api -- openapi > backend/openapi.json` and `git diff --exit-code`. It writes through `std::io::Write`, because `print_stdout` is linted ([rust-cargo-lints](../rust-cargo-lints/SKILL.md)).
- **utoipa 6 changes** that matter here: OpenAPI 3.2 is opt-in, expression `ignore` is gone from `ToSchema`/`IntoParams`, YAML moved to `yaml_serde` ([utoipa CHANGELOG 6.0.0](https://docs.rs/crate/utoipa/6.0.0/source/CHANGELOG.md)).

The block documents `GET /me` ([contracts 9, account](../../../docs/architecture/contracts.md#account)) and the scan event stream. Its handlers are stubs, so they take no extractors; the real event handler, with `ApiPath<ScanId>`, is in [sse 3](sse.md#3-the-compiled-example). `ErrorBody` here is the spec's view of the body that [error-responses 2](error-responses.md#2-the-compiled-example) writes, with every contracts 2 field and code.

```rust
use std::io::{self, Write};

use axum::{Json, http::StatusCode};
use serde::Serialize;
use utoipa::{
    Modify, OpenApi, ToSchema,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_axum::{router::OpenApiRouter, routes};

// copy of: rust-axum-api/error-responses.md ErrorCode
/// contracts 2 `ErrorCode`. In the domain crate; `ToSchema` adds it to the generated spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    Unauthenticated,
    Forbidden,
    InvalidSignature,
    NotFound,
    InvalidState,
    RequestInProgress,
    LeaseLost,
    UploadTooLarge,
    UnsupportedMediaType,
    ImageQuality,
    MissingRequiredShot,
    TooManyImages,
    IdempotencyConflict,
    RateLimited,
    BudgetExceeded,
    Internal,
    ProcessingUnavailable,
    UpstreamUnavailable,
}
// end: ErrorCode

/// contracts 2 `ErrorBody`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorObject,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorObject {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    /// Optional, never `null` (contracts 1, notation).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub retry_after_s: Option<u64>,
    /// Per code (contracts 2); optional, never `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object, nullable = false)]
    pub details: Option<serde_json::Value>,
    pub request_id: String,
}

/// contracts 9, account: the `GET /me` response.
#[derive(Debug, Serialize, ToSchema)]
pub struct Me {
    pub id: String,
    pub display_name: String,
    pub location_consent: bool,
    pub created_at: Timestamp,
}

/// Every app route sends a Clerk session token (contracts 1, authentication headers).
pub struct ClerkBearer;

impl Modify for ClerkBearer {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let scheme = HttpBuilder::new()
            .scheme(HttpAuthScheme::Bearer)
            .bearer_format("JWT")
            .build();
        openapi
            .components
            .get_or_insert_with(Default::default)
            .add_security_scheme("clerk", SecurityScheme::Http(scheme));
    }
}

#[utoipa::path(
    get,
    path = "/me",
    tag = "account",
    responses(
        (status = 200, description = "The signed-in user", body = Me),
        (status = 401, description = "unauthenticated", body = ErrorBody),
    )
)]
pub async fn me() -> Json<Me> {
    Json(Me {
        id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".to_owned(),
        display_name: "Sam".to_owned(),
        location_consent: false,
        created_at: chrono::DateTime::from_timestamp(1_792_281_600, 0)
            .expect("a valid timestamp")
            .into(),
    })
}

#[utoipa::path(
    get,
    path = "/scans/{id}/events",
    tag = "scans",
    params(
        ("id" = String, Path, description = "Scan ID (ULID)"),
        ("Last-Event-ID" = Option<u64>, Header, description = "Last applied seq"),
    ),
    responses(
        (status = 200, description = "Scan events (contracts 4)", content_type = "text/event-stream", body = String),
        (status = 400, description = "invalid_request", body = ErrorBody),
        (status = 401, description = "unauthenticated", body = ErrorBody),
        (status = 404, description = "not_found", body = ErrorBody),
    )
)]
pub async fn scan_events() -> StatusCode {
    StatusCode::OK
}

#[derive(OpenApi)]
#[openapi(
    info(title = "PDM API", version = "1"),
    components(schemas(ErrorBody)),
    modifiers(&ClerkBearer),
    security(("clerk" = []))
)]
pub struct ApiDoc;

pub fn router() -> (axum::Router, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(me))
        .routes(routes!(scan_events))
        .split_for_parts()
}

/// `api openapi`: prints the spec for `backend/openapi.json`.
pub fn write_openapi(mut out: impl Write) -> io::Result<()> {
    let (_, spec) = router();
    let json = spec.to_pretty_json().map_err(io::Error::other)?;
    writeln!(out, "{json}")
}

// copy of: rust-contract-types/primitives.md Timestamp
/// RFC 3339 in UTC, whole seconds, `Z` suffix: `"2026-10-18T14:03:22Z"` (contracts 1).
/// Keeps full precision in memory; only the wire form is truncated. Whole seconds are
/// this skill's choice, not the contracts' (contracts 12, question 12).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, utoipa::ToSchema, schemars::JsonSchema,
)]
#[schema(value_type = String, format = DateTime, example = "2026-10-18T14:03:22Z")]
#[schemars(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);

impl Timestamp {
    pub fn now() -> Self {
        Self(chrono::Utc::now())
    }

    /// For the store, which binds `chrono::DateTime<Utc>` (database 2).
    pub const fn into_inner(self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(t: chrono::DateTime<chrono::Utc>) -> Self {
        Self(t)
    }
}

impl serde::Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = self.0.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        serializer.serialize_str(&text)
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    /// Accepts what chrono's RFC 3339 parser accepts and converts it to UTC.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <chrono::DateTime<chrono::Utc> as serde::Deserialize>::deserialize(deserializer).map(Self)
    }
}
// end: Timestamp

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn spec() -> Value {
        let mut out = Vec::new();
        write_openapi(&mut out).unwrap();
        serde_json::from_slice(&out).unwrap()
    }

    #[test]
    fn spec_lists_routes_errors_and_the_sse_content_type() {
        let spec = spec();
        assert_eq!(spec["openapi"], "3.1.0");
        let me = &spec["paths"]["/me"]["get"];
        assert_eq!(
            me["responses"]["401"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/ErrorBody"
        );
        let events = &spec["paths"]["/scans/{id}/events"]["get"]["responses"]["200"];
        assert!(events["content"]["text/event-stream"].is_object());
    }

    #[test]
    fn every_route_requires_the_clerk_bearer_token() {
        let spec = spec();
        assert_eq!(
            spec["components"]["securitySchemes"]["clerk"],
            json!({ "type": "http", "scheme": "bearer", "bearerFormat": "JWT" })
        );
        assert_eq!(spec["security"], json!([{ "clerk": [] }]));
    }

    #[test]
    fn the_error_code_enum_has_every_contract_code() {
        let spec = spec();
        let codes = &spec["components"]["schemas"]["ErrorCode"]["enum"];
        assert_eq!(codes.as_array().map(Vec::len), Some(19));
        assert_eq!(codes[0], "invalid_request");
        assert_eq!(codes[18], "upstream_unavailable");
    }

    #[test]
    fn optional_fields_are_not_nullable() {
        let spec = spec();
        let object = &spec["components"]["schemas"]["ErrorObject"];
        let required: Vec<&str> = object["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(required, ["code", "message", "retryable", "request_id"]);
        assert_eq!(
            object["properties"]["retry_after_s"]["type"],
            Value::from("integer")
        );
    }

    #[tokio::test]
    async fn the_router_serves_what_the_spec_lists() {
        use tower::ServiceExt;
        let (app, _) = router();
        let request = axum::http::Request::get("/me")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
```

What the tests pin down: the document is OpenAPI 3.1.0; an error status references `ErrorBody`, whose `ErrorCode` lists all 19 codes; the SSE route is `text/event-stream`; every route requires the `clerk` HTTP bearer scheme (a [`Modify`](https://docs.rs/utoipa/6.0.0/utoipa/trait.Modify.html) adds the [`SecurityScheme::Http`](https://docs.rs/utoipa/6.0.0/utoipa/openapi/security/enum.SecurityScheme.html) component and `security(...)` applies it to every operation, [`OpenApi` derive](https://docs.rs/utoipa/6.0.0/utoipa/derive.OpenApi.html)), so the generated client knows to send `Authorization: Bearer` ([contracts 1, authentication headers](../../../docs/architecture/contracts.md#authentication-headers)); an `Option` field marked `#[schema(nullable = false)]` with `skip_serializing_if` is optional and not nullable, which is what makes `openapi-typescript` emit `retry_after_s?: number` rather than `number | null` ([`ToSchema`, field nullability and required rules](https://docs.rs/utoipa/6.0.0/utoipa/derive.ToSchema.html#field-nullability-and-required-rules)).
