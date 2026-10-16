//! Extractors whose rejection is an [`ApiError`], so every rejection gets a contracts 2 body,
//! never axum's plain text: the JSON, query and path wrappers here, and the auth, access and
//! idempotency extractors in the submodules.
//!
//! Every extractor is generic over the router's state `S` with the bound
//! `Arc<AppState>: FromRef<S>`: the extractor takes what it needs out of any state that can
//! produce an `Arc<AppState>`, so it doesn't fix the router's state type.

pub mod access;
pub mod auth;
pub mod idempotency;

use std::str::FromStr;

use axum::{
    extract::{
        FromRequestParts, RawPathParams,
        multipart::MultipartRejection,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::request::Parts,
};

use crate::error::ApiError;

/// `axum::Json` with contracts 2 rejections: `400 invalid_request`, `field` = the JSON path.
#[derive(Debug, axum::extract::FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct ApiJson<T>(pub T);

/// `axum::extract::Query` with contracts 2 rejections.
#[derive(Debug, axum::extract::FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct ApiQuery<T>(pub T);

/// `axum::extract::Path` with contracts 2 rejections: a capture that doesn't parse (an id that
/// isn't a ULID) is `404 not_found`, like any unknown id.
#[derive(Debug, axum::extract::FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct ApiPath<T>(pub T);

/// `axum::extract::Multipart` with contracts 2 rejections.
pub struct ApiMultipart(pub axum::extract::Multipart);

impl<S: Send + Sync> axum::extract::FromRequest<S> for ApiMultipart {
    type Rejection = ApiError;

    async fn from_request(request: axum::extract::Request, state: &S) -> Result<Self, ApiError> {
        Ok(Self(
            axum::extract::Multipart::from_request(request, state).await?,
        ))
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        type PathError = serde_path_to_error::Error<serde_json::Error>;
        let found = std::iter::successors(
            Some(&rejection as &(dyn std::error::Error + 'static)),
            |e| (*e).source(),
        )
        .find_map(|e| e.downcast_ref::<PathError>());
        match (&rejection, found) {
            (_, Some(e)) => {
                let path = e.path().to_string();
                let field = if path == "." { "body".to_owned() } else { path };
                Self::invalid_request(field, e.inner().to_string())
            }
            (JsonRejection::MissingJsonContentType(_), None) => {
                Self::invalid_request("content-type", "expected application/json")
            }
            (_, None) => Self::invalid_request("body", rejection.body_text()),
        }
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        type PathError = serde_path_to_error::Error<serde::de::value::Error>;
        let found = std::iter::successors(
            Some(&rejection as &(dyn std::error::Error + 'static)),
            |e| (*e).source(),
        )
        .find_map(|e| e.downcast_ref::<PathError>());
        match found {
            Some(e) => Self::invalid_request(e.path().to_string(), e.inner().to_string()),
            None => Self::invalid_request("query", rejection.body_text()),
        }
    }
}

impl From<PathRejection> for ApiError {
    fn from(rejection: PathRejection) -> Self {
        match rejection {
            PathRejection::FailedToDeserializePathParams(_) => Self::not_found(),
            other => Self::internal(other),
        }
    }
}

impl From<MultipartRejection> for ApiError {
    fn from(rejection: MultipartRejection) -> Self {
        Self::invalid_request("content-type", rejection.body_text())
    }
}

impl From<axum::extract::multipart::MultipartError> for ApiError {
    fn from(e: axum::extract::multipart::MultipartError) -> Self {
        if e.status().as_u16() == 413 {
            Self::new(domain::error::ErrorKind::UploadTooLarge {
                max_bytes: domain::rules::uploads::IMAGE_MAX_BYTES,
            })
        } else {
            Self::invalid_request("body", e.body_text())
        }
    }
}

/// Reads one named capture of the route (`{id}`, `{image_id}`) and parses it. A capture that
/// doesn't parse is `404`: nothing can exist under it.
pub async fn path_param<T: FromStr, S: Send + Sync>(
    parts: &mut Parts,
    state: &S,
    name: &str,
) -> Result<T, ApiError> {
    let params = RawPathParams::from_request_parts(parts, state)
        .await
        .map_err(ApiError::internal)?;
    params
        .iter()
        .find(|(key, _)| *key == name)
        .and_then(|(_, value)| value.parse().ok())
        .ok_or_else(ApiError::not_found)
}

/// The bearer token of `Authorization: Bearer …`.
pub fn bearer(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

/// The request's `req_<ULID>`, set by the outermost layer.
pub fn request_id(parts: &Parts) -> String {
    parts
        .extensions
        .get::<tower_http::request_id::RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("req_unknown")
        .to_owned()
}

/// The request's `req_<ULID>` as an extractor, for handlers that begin an idempotency record by
/// hand (the multipart uploads).
#[derive(Debug, Clone)]
pub struct RequestIdHeader(pub String);

impl<S: Send + Sync> FromRequestParts<S> for RequestIdHeader {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(Ok(Self(request_id(parts))))
    }
}
