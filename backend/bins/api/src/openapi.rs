//! The `OpenAPI` document's frame: title, version, tags and the bearer scheme. The paths and
//! schemas come from the handlers' `#[utoipa::path]` through `OpenApiRouter`.

use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

/// The document's frame. `OpenAPI` 3.1 (utoipa 6's default); every path needs the Clerk bearer
/// token unless it declares `security(())` (signed media, ops, the webhook) or is a worker route.
#[derive(OpenApi)]
#[openapi(
    info(title = "PDM API", description = "Plant, crop and garden health monitoring."),
    modifiers(&BearerAuth),
    security(("clerk" = [])),
    // Types no path names: `PushPayload` is the `data` of a push message, which the app reads.
    components(schemas(
        domain::chat::AnchorType,
        domain::records::task::TaskState,
        domain::garden::push::PushPayload
    )),
    tags(
        (name = "scans", description = "contracts 3 and 4"),
        (name = "gardens", description = "contracts 9"),
        (name = "members", description = "contracts 9, members and invites"),
        (name = "map", description = "contracts 9, garden map"),
        (name = "plants", description = "contracts 9, plants"),
        (name = "species", description = "contracts 9, species"),
        (name = "tasks", description = "contracts 9, tasks"),
        (name = "account", description = "contracts 9, account"),
        (name = "memories", description = "contracts 8, memory"),
        (name = "chat", description = "contracts 8"),
        (name = "connectors", description = "farm connectors, docs/connectors 5 and 11"),
        (name = "media", description = "signed image URLs, storage 3"),
        (name = "worker", description = "contracts 5; worker token"),
        (name = "ops", description = "contracts 9, ops and auth webhook"),
    )
)]
pub struct ApiDoc;

/// Adds the `clerk` bearer scheme (a Clerk session token) and the `worker` scheme.
struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "clerk",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
        components.add_security_scheme(
            "worker",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
    }
}
