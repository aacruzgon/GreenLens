//! Images through the API's own signed URLs ([storage 3]): verify `exp` and `sig` (HMAC over
//! `"GET\n{path}\n{exp}"`), look up the record's storage key, then stream the object from the
//! blob store without buffering, with `Content-Type`, `ETag` and
//! `Cache-Control: private, max-age=900`. Depth maps have no route.

use axum::{
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures::StreamExt as _;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use domain::{
    error::ErrorKind,
    ports::{BlobStore, ObjectRef},
    url::{ImagePath, verify_signed},
};

use crate::{error::ApiError, state::AppState};

/// `?exp=&sig=` of a signed URL.
#[derive(Debug, Clone, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SignedQuery {
    /// Expiry, Unix seconds.
    pub exp: i64,
    /// HMAC-SHA256, lowercase hex.
    pub sig: String,
}

/// `403 invalid_signature`.
pub fn invalid_signature() -> ApiError {
    ApiError::new(ErrorKind::InvalidSignature)
}

/// Checks the signature for `path` at the current time.
pub fn verify(state: &AppState, path: &ImagePath, query: &SignedQuery) -> Result<(), ApiError> {
    verify_signed(
        &*state.ports.signer,
        path,
        query.exp,
        &query.sig,
        state.ports.clock.now(),
    )
    .map_err(|_| invalid_signature())
}

/// Streams an object. The body is the blob store's `'static` chunk stream handed straight to
/// hyper: if the client goes away, dropping it closes the connection to `RustFS`. It ends when
/// `shutdown` fires, because the route's `TimeoutLayer` bounds only the response head and a slow
/// download would otherwise hold `axum::serve`'s graceful shutdown open; the client sees a short
/// body and fetches the image again from the next process.
pub async fn stream_object(
    blob: &dyn BlobStore,
    object: ObjectRef,
    shutdown: &CancellationToken,
) -> Result<Response, ApiError> {
    let fetched = blob.get(&object.key).await?;
    let body = fetched.body.take_until(shutdown.clone().cancelled_owned());
    let mut response = (StatusCode::OK, Body::from_stream(body)).into_response();
    let headers = response.headers_mut();
    let content_type = HeaderValue::from_str(&fetched.content_type)
        .unwrap_or_else(|_| HeaderValue::from_static(object.content_type.as_str()));
    headers.insert(header::CONTENT_TYPE, content_type);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=900"),
    );
    if let Some(etag) = fetched.etag.and_then(|e| HeaderValue::from_str(&e).ok()) {
        // The S3 ETag already carries its quotes; passed through verbatim.
        headers.insert(header::ETAG, etag);
    }
    if let Some(length) = fetched.length {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;
    use http_body_util::BodyExt as _;

    use domain::ports::{BlobObject, ContentType, MockBlobStore, ObjectKey};

    use super::*;

    #[tokio::test]
    async fn an_image_body_ends_on_the_shutdown_token() {
        let mut blob = MockBlobStore::new();
        // One chunk, then a client or RustFS too slow to ever finish.
        blob.expect_get().returning(|_| {
            Ok(BlobObject {
                content_type: "image/webp".to_owned(),
                length: None,
                etag: None,
                body: futures::stream::once(async { Ok(Bytes::from_static(b"RIFF")) })
                    .chain(futures::stream::pending())
                    .boxed(),
            })
        });
        let object = ObjectRef {
            key: ObjectKey::from_stored("gardens/01JAX3K9T6R2B8V0QZ4M7D1E5G/scans/01JAX3K9T6R2B8V0QZ4M7D1E5S/01JAX3K9T6R2B8V0QZ4M7D1E5A.webp").unwrap(),
            content_type: ContentType::Webp,
        };
        let shutdown = CancellationToken::new();
        let response = stream_object(&blob, object, &shutdown).await.unwrap();
        let mut body = response.into_body();
        let first = body.frame().await.unwrap().unwrap();
        assert_eq!(first.data_ref().unwrap().as_ref(), b"RIFF");
        shutdown.cancel();
        let rest = tokio::time::timeout(Duration::from_secs(5), body.collect())
            .await
            .expect("the body ends when shutdown starts")
            .unwrap();
        assert!(rest.to_bytes().is_empty());
    }
}
