//! `Wire<T>`: every JSON success response. Its bound is `T: Serialize + utoipa::ToSchema`, and
//! only the signed form of a URL-bearing type implements `ToSchema` (`domain::url`), so a handler
//! that tries to return an unsigned record (`Scan<Stored>`) doesn't compile.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

use domain::ports::StoredResponse;

use crate::error::ApiError;

/// A status and a JSON body.
#[derive(Debug, Clone)]
pub struct Wire<T> {
    status: StatusCode,
    body: T,
}

impl<T: Serialize + ToSchema> Wire<T> {
    /// `200`.
    pub fn ok(body: T) -> Self {
        Self {
            status: StatusCode::OK,
            body,
        }
    }

    /// `201`.
    pub fn created(body: T) -> Self {
        Self {
            status: StatusCode::CREATED,
            body,
        }
    }

    /// `202`.
    pub fn accepted(body: T) -> Self {
        Self {
            status: StatusCode::ACCEPTED,
            body,
        }
    }

    /// With an explicit status (`200` or `202` for submit).
    pub fn with_status(status: StatusCode, body: T) -> Self {
        Self { status, body }
    }

    /// The form the idempotency record stores.
    pub fn stored(&self) -> Result<StoredResponse, ApiError> {
        Ok(StoredResponse {
            status: self.status.as_u16(),
            body: serde_json::to_value(&self.body).map_err(ApiError::internal)?,
        })
    }
}

impl<T: Serialize + ToSchema> IntoResponse for Wire<T> {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

/// A stored response replayed as-is ([contracts 1]: "the stored response is returned with its
/// original status").
pub fn replay(stored: StoredResponse) -> Response {
    let status = StatusCode::from_u16(stored.status).unwrap_or(StatusCode::OK);
    (status, Json(stored.body)).into_response()
}
