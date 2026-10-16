# Mocks and fakes

Part of [`rust-testing`](SKILL.md). A mock stands in for a boundary trait from `domain` ([architecture 2, design principles](../../../docs/architecture/README.md#design-principles)); the trait shapes are decided in [`rust-async`](../rust-async/traits.md): `fn m(&self, …) -> BoxFuture<'_, Result<T, E>>` for a trait held as `Arc<dyn Trait>` in `AppState`, `-> impl Future<Output = Result<T, E>> + Send` for one used only through generics. mockall 0.15.0 mocks both.

## A mock with mockall

`#[automock]` generates `MockWeatherProvider`; each method gets `expect_<method>()`, with `.withf(…)` to match the arguments, `.times(n)` for the call count and `.returning(…)` for the result. An unmet `.times` panics when the mock is dropped ([mockall, getting started](https://docs.rs/mockall/0.15.0/mockall/index.html#getting-started); [call counts](https://docs.rs/mockall/0.15.0/mockall/index.html#call-counts)).

```rust
use std::sync::Arc;

use futures::future::BoxFuture;
use mockall::automock;

#[derive(Debug, PartialEq)]
struct Forecast {
    temp_c: f64,
}

#[derive(Debug, PartialEq)]
enum PlanError {
    UpstreamUnavailable { upstream: &'static str },
}

#[automock]
trait WeatherProvider: Send + Sync {
    fn forecast(&self, garden_id: &str) -> BoxFuture<'_, Result<Forecast, String>>;
}

struct AppState {
    weather: Arc<dyn WeatherProvider>,
}

/// Maps a provider failure to the contracts §2 `upstream_unavailable` details.
async fn weather_for(state: &AppState, garden_id: &str) -> Result<Forecast, PlanError> {
    state
        .weather
        .forecast(garden_id)
        .await
        .map_err(|_| PlanError::UpstreamUnavailable {
            upstream: "weather",
        })
}

#[tokio::test]
async fn provider_failure_is_upstream_unavailable() {
    let mut weather = MockWeatherProvider::new();
    weather
        .expect_forecast()
        .withf(|garden_id| garden_id == "01JAX3K9T6R2B8V0QZ4M7D1E5F")
        .times(1)
        .returning(|_| Box::pin(async { Err("timeout".to_owned()) }));
    let state = AppState {
        weather: Arc::new(weather),
    };

    let result = weather_for(&state, "01JAX3K9T6R2B8V0QZ4M7D1E5F").await;
    assert_eq!(
        result,
        Err(PlanError::UpstreamUnavailable {
            upstream: "weather"
        })
    );
}
```

- ✓ The mock goes into `Arc<dyn WeatherProvider>` as it is, so a handler test can put it in the test `AppState`.
- ✓ The same `.returning(|_| Box::pin(async { … }))` works for a method declared `-> impl Future<Output = T> + Send`: mockall turns an `impl Future` return into a pinned, boxed future ([mockall, impl Future](https://docs.rs/mockall/0.15.0/mockall/index.html#impl-future)). The `BlobStore` mock below proves it.
- mockall also accepts native `async fn` in traits, where `.returning` gives the output value directly ([mockall, async traits](https://docs.rs/mockall/0.15.0/mockall/index.html#async-traits)). ✓ But a `pub` trait with `async fn` warns with rustc's `async_fn_in_trait`, and `rust-async` rules that form out for `domain`.

## A fake for state

When a test needs state that lasts across calls (put, then get; delete twice), a scripted mock grows one expectation per call and stops describing behaviour. Write a small in-memory fake behind the trait instead, and give it the real store's rules, or the test proves nothing. This is this skill's choice; no source rules on it. For `BlobStore` the rules are in [storage 4](../../../docs/storage/README.md#4-writes-and-deletes): a put never overwrites a key (`If-None-Match: *`; a second put fails with `412 PreconditionFailed`), and deleting a missing key is a no-op. Database behaviour is never faked: store code is tested against SurrealDB ([database.md](database.md)).

`BlobStore` returns `impl Future<Output = …> + Send`, the form for a trait used through generics ([`rust-async`, traits](../rust-async/traits.md)). The fake does each call's work at once and returns [`std::future::ready`](https://doc.rust-lang.org/std/future/fn.ready.html); an `async fn` with no `.await` would trip Clippy's [`unused_async_trait_impl`](https://clippy.rust-lang.org/rust-1.99.0/index.html#unused_async_trait_impl). The fake's error is the `blob` crate's `BlobError`, an exact copy of the canonical one in [rust-errors, mapping.md](../rust-errors/mapping.md); a refused put is `Rejected { operation: "put", .. }` with the `PreconditionFailed` code RustFS sends, built with the SDK's `generic` constructor ([`ErrorMetadata`](https://docs.rs/aws-sdk-s3/1.152.0/aws_sdk_s3/error/struct.ErrorMetadata.html)). The second test mocks the same trait: mockall turns each `impl Future` return into a pinned, boxed future, so `.returning` gives `Box::pin(async { … })` ([mockall, impl Future](https://docs.rs/mockall/0.15.0/mockall/index.html#impl-future)).

```rust
use std::collections::HashMap;
use std::sync::Mutex;

use mockall::automock;

#[automock]
trait BlobStore: Send + Sync {
    fn put(&self, key: &str, bytes: Vec<u8>) -> impl Future<Output = Result<(), BlobError>> + Send;
    fn get(&self, key: &str) -> impl Future<Output = Option<Vec<u8>>> + Send;
    fn delete(&self, key: &str) -> impl Future<Output = Result<(), BlobError>> + Send;
}

/// In-memory `BlobStore` that keeps the storage §4 rules.
#[derive(Default)]
struct FakeBlobs {
    objects: Mutex<HashMap<String, Vec<u8>>>,
}

/// The object store's answer to a put on an existing key (storage 4).
fn precondition_failed() -> aws_sdk_s3::Error {
    let meta = aws_sdk_s3::error::ErrorMetadata::builder()
        .code("PreconditionFailed")
        .build();
    aws_sdk_s3::operation::put_object::PutObjectError::generic(meta).into()
}

// Each method does its work at once and returns a ready future.
impl BlobStore for FakeBlobs {
    fn put(&self, key: &str, bytes: Vec<u8>) -> impl Future<Output = Result<(), BlobError>> + Send {
        let mut objects = self.objects.lock().expect("fake lock");
        let result = if objects.contains_key(key) {
            Err(BlobError::Rejected {
                operation: "put",
                source: precondition_failed(),
            })
        } else {
            objects.insert(key.to_owned(), bytes);
            Ok(())
        };
        std::future::ready(result)
    }

    fn get(&self, key: &str) -> impl Future<Output = Option<Vec<u8>>> + Send {
        std::future::ready(self.objects.lock().expect("fake lock").get(key).cloned())
    }

    fn delete(&self, key: &str) -> impl Future<Output = Result<(), BlobError>> + Send {
        self.objects.lock().expect("fake lock").remove(key);
        std::future::ready(Ok(()))
    }
}

/// Code under test: generic over the trait, as `rust-async` prescribes for this form.
async fn store_once<B: BlobStore>(blobs: &B, key: &str, bytes: Vec<u8>) -> &'static str {
    match blobs.put(key, bytes).await {
        Ok(()) => "stored",
        Err(BlobError::Rejected { .. }) => "exists",
        Err(BlobError::Unavailable(_)) => "unavailable",
    }
}

// copy of: rust-errors/mapping.md BlobError
/// The `blob` crate's error.
#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("object store unreachable")]
    Unavailable(#[source] aws_sdk_s3::Error),
    #[error("object store rejected {operation}")]
    Rejected {
        operation: &'static str,
        #[source]
        source: aws_sdk_s3::Error,
    },
}
// end: BlobError

const KEY: &str = "gardens/01JAX3K9T6R2B8V0QZ4M7D1E5F/scans/01JAX3M2W7C5N9P4R8T6V0X2Y4/01JAX3N8D3F7H1K5M9Q3S7V1W5.webp";

#[tokio::test]
async fn the_fake_keeps_the_storage_rules() {
    let blobs = FakeBlobs::default();
    assert_eq!(store_once(&blobs, KEY, b"webp".to_vec()).await, "stored");
    let again = blobs.put(KEY, b"other".to_vec()).await.unwrap_err();
    let BlobError::Rejected { operation, source } = again else {
        panic!("a second put is refused, got {again:?}");
    };
    assert_eq!(operation, "put");
    assert_eq!(
        aws_sdk_s3::error::ProvideErrorMetadata::code(&source),
        Some("PreconditionFailed")
    );
    assert_eq!(blobs.get(KEY).await.as_deref(), Some(&b"webp"[..]));
    blobs.delete(KEY).await.unwrap();
    blobs.delete(KEY).await.unwrap(); // a missing key is a no-op
    assert_eq!(blobs.get(KEY).await, None);
}

#[tokio::test]
async fn an_impl_future_method_is_mocked_with_a_pinned_box() {
    let mut blobs = MockBlobStore::new();
    blobs
        .expect_put()
        .withf(|key, _| key == KEY)
        .times(1)
        .returning(|_, _| {
            Box::pin(async {
                Err(BlobError::Rejected {
                    operation: "put",
                    source: precondition_failed(),
                })
            })
        });
    assert_eq!(store_once(&blobs, KEY, Vec::new()).await, "exists");
}
```

The guard is never held across an `.await` (each method locks, works and returns a ready future), which is what [`await_holding_lock`](https://clippy.rust-lang.org/rust-1.99.0/index.html#await_holding_lock) checks. The key follows the layout in [storage 2](../../../docs/storage/README.md#2-bucket-keys-and-access).
