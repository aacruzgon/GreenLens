//! Hand-written `OpenAPI` (`utoipa`) and JSON Schema (`schemars`) for the types whose serde form
//! is a plain string, number or literal, so the generated specs show what the wire carries.

use utoipa::openapi::{
    RefOr,
    schema::{ObjectBuilder, Schema, Type},
};

/// A `string` schema with an optional regular-expression pattern and a description.
pub(crate) fn string(pattern: Option<&str>, description: &str) -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(Type::String)
        .pattern(pattern)
        .description(Some(description))
        .into()
}

/// A JSON Schema `string` with an optional pattern, for `schemars`.
pub(crate) fn json_string(pattern: Option<&str>, description: &str) -> schemars::Schema {
    let mut schema = schemars::json_schema!({ "type": "string", "description": description });
    if let Some(pattern) = pattern {
        schema.insert("pattern".to_owned(), pattern.into());
    }
    schema
}

/// Implements `utoipa::ToSchema` and `schemars::JsonSchema` for a type serialised as a string.
macro_rules! string_schema {
    ($ty:ty, $name:expr, $pattern:expr, $description:expr) => {
        impl utoipa::PartialSchema for $ty {
            fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
                $crate::schema_util::string($pattern, $description)
            }
        }
        impl utoipa::ToSchema for $ty {
            fn name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed($name)
            }
        }
        impl schemars::JsonSchema for $ty {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed($name)
            }
            fn inline_schema() -> bool {
                true
            }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                $crate::schema_util::json_string($pattern, $description)
            }
        }
    };
}
pub(crate) use string_schema;
