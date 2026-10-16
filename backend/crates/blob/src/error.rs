//! `SdkError` to `BlobError`, the one place S3 errors are classified (research `http-s3.md` §9).

use aws_sdk_s3::{
    config::http::HttpResponse,
    error::{DisplayErrorContext, ProvideErrorMetadata, SdkError},
};
use domain::ports::BlobError;

/// Maps an SDK error of `operation`.
///
/// - `412 PreconditionFailed` (an `If-None-Match: *` put of an existing key) arrives as the
///   operation's `Unhandled` variant in 1.152, so it is matched by status or code, never by
///   variant ✓ (run against `RustFS` 1.0.1).
/// - `404` / `NoSuchKey` is `NotFound`.
/// - Timeouts, dispatch and response failures and `5xx` are `Unavailable`; anything else is
///   `Rejected` (a policy, a bad key).
///
/// The SDK error stays the `source`, so the log keeps its chain; `DisplayErrorContext` prints
/// the whole chain (a plain `to_string()` is just "service error"). It includes the raw
/// response, so it is logged, never returned to a client.
pub fn map_sdk_error<E>(operation: &'static str, error: SdkError<E, HttpResponse>) -> BlobError
where
    E: ProvideErrorMetadata + std::error::Error + Send + Sync + 'static,
{
    let status = error.raw_response().map(|r| r.status().as_u16());
    let code = error.code().map(str::to_owned);
    match (status, code.as_deref()) {
        (Some(412), _) | (_, Some("PreconditionFailed")) => return BlobError::AlreadyExists,
        (Some(404), _) | (_, Some("NoSuchKey" | "NotFound")) => return BlobError::NotFound,
        _ => {}
    }
    tracing::warn!(operation, error = %DisplayErrorContext(&error), "storage call failed");
    let transient = matches!(
        error,
        SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) | SdkError::ResponseError(_)
    ) || status.is_some_and(|s| s >= 500);
    if transient {
        BlobError::Unavailable(Box::new(error))
    } else {
        BlobError::Rejected {
            operation,
            source: Box::new(error),
        }
    }
}
