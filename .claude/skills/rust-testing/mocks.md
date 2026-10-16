# Mocks and fakes

Part of [`rust-testing`](SKILL.md). A mock stands in for a port, a boundary trait in `domain::ports` ([architecture 2, design principles](../../../docs/architecture/README.md#design-principles); [backend domain 9](../../../docs/backend/domain.md#9-boundary-traits)). How the ports are written is decided in [rust-async, traits 3](../rust-async/traits.md#3-what-the-project-does-with-its-boundary-traits): `#[async_trait]` on the trait and every `impl`, held as `Arc<dyn Port>`, with `#[mockall::automock]` above `#[async_trait::async_trait]`. That page has the reasons and the compile-fail proofs; this one shows how tests use the result.

## What `domain` provides

The backend doesn't write its mocks by hand ([backend domain 11](../../../docs/backend/domain.md#11-testing)):

```toml
[dev-dependencies]
domain = { workspace = true, features = ["mock", "test-util"] }
```

- **`mock`** generates `Mock<Trait>` for every port except `ChatModel` (`#[cfg_attr(feature = "mock", mockall::automock)]` on each, as the `BlobStore` port shows in [backend domain 9, object storage](../../../docs/backend/domain.md#object-storage)). `ChatModel::stream` returns a `BoxStream<'a, …>` borrowing its request, which mockall can't mock ([rust-async, traits 3](../rust-async/traits.md#3-what-the-project-does-with-its-boundary-traits), rule 6).
- **`test-util`** adds the hand-written doubles: `FixedClock` and `SequentialIds` for the synchronous `Clock` and `IdGen` ports, `FakeSigner` and `FixedPhotos` for URL signing and species photos, and `ScriptedChat` for `ChatModel`.
- **Fakes with the real rules** live in the crate that owns the real implementation: `blob`'s `MemoryBlobStore` follows `RustFS`'s rules ([backend blob 8](../../../docs/backend/blob.md#8-the-in-memory-store)). For the store there are two answers. Its own tests run its SQL on the embedded engine ([database.md](database.md)). Code above it mocks the area traits: there is no `MockStore`, since `Store` is a supertrait with a blanket impl, so `mock` gives one mock per repository trait ([backend domain 9](../../../docs/backend/domain.md#9-boundary-traits)), and `api`'s handler tests combine them into `FakeStore` (`bins/api/src/tests/fake_store.rs`, generated from the traits by `backend/tools/apigen/gen_fake_store.py`, every method delegated to its area's mock, default-bodied ones included; [backend api 21](../../../docs/backend/api.md#21-testing)). That "fake" is mocks underneath, so a test sets expectations on it; it keeps no state.

The blocks below define small stand-ins for those ports, because the checker builds no `domain` crate.

## A mock with mockall

`#[automock]` generates `MockWeatherProvider`; each method gets `expect_<method>()`, with `.withf(…)` to match the arguments, `.times(n)` for the call count and `.returning(…)` for the result. An unmet `.times` panics when the mock is dropped ([mockall, getting started](https://docs.rs/mockall/0.15.0/mockall/index.html#getting-started); [call counts](https://docs.rs/mockall/0.15.0/mockall/index.html#call-counts)). Above `#[async_trait]`, mockall sees the trait's `async fn` and `.returning` gives the output value itself, not a future ([mockall, async traits](https://docs.rs/mockall/0.15.0/mockall/index.html#async-traits)).

```rust
use std::sync::Arc;

#[derive(Debug, PartialEq)]
struct Forecast {
    temp_c: f64,
}

/// Stands in for `domain::ports::ProviderError` (backend domain 6).
#[derive(Debug, PartialEq, thiserror::Error)]
#[error("{provider}: unavailable")]
struct ProviderUnavailable {
    provider: &'static str,
}

#[derive(Debug, PartialEq)]
enum PlanError {
    UpstreamUnavailable { upstream: &'static str },
}

/// A port: `#[automock]` above `#[async_trait]`, `Send + Sync`, held as `Arc<dyn …>`.
#[mockall::automock]
#[async_trait::async_trait]
trait WeatherProvider: Send + Sync {
    async fn forecast(&self, garden_id: &str) -> Result<Forecast, ProviderUnavailable>;
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
        .map_err(|e| PlanError::UpstreamUnavailable {
            upstream: e.provider,
        })
}

#[tokio::test]
async fn provider_failure_is_upstream_unavailable() {
    let mut weather = MockWeatherProvider::new();
    weather
        .expect_forecast()
        .withf(|garden_id| garden_id == "01JAX3K9T6R2B8V0QZ4M7D1E5F")
        .times(1)
        .returning(|_| {
            Err(ProviderUnavailable {
                provider: "weather",
            })
        });
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

#[tokio::test]
async fn a_mock_behind_dyn_runs_in_a_spawned_task() {
    let mut weather = MockWeatherProvider::new();
    weather
        .expect_forecast()
        .returning(|_| Ok(Forecast { temp_c: -1.5 }));
    let weather: Arc<dyn WeatherProvider> = Arc::new(weather);
    let forecast = tokio::spawn(async move { weather.forecast("g1").await })
        .await
        .expect("no panic");
    assert_eq!(forecast, Ok(Forecast { temp_c: -1.5 }));
}
```

- ✓ The mock goes into `Arc<dyn WeatherProvider>` as it is, so a handler test puts it in the test `AppState` ([handlers.md](handlers.md)), and its future is `Send`, so it runs in `tokio::spawn`.
- Mock only a port. Don't add a trait just to mock a function ([SKILL.md, section 6](SKILL.md#6-mocks-and-fakes)).

## A fake for state

When a test needs state that lasts across calls (put, then get; delete twice), a scripted mock grows one expectation per call and stops describing behaviour. A small in-memory fake behind the port does better, if it keeps the real store's rules, or the test proves nothing. This is this skill's choice, and the design's: `blob` ships `MemoryBlobStore` for exactly this ([backend blob 8](../../../docs/backend/blob.md#8-the-in-memory-store)). For object storage the rules are in [storage 4](../../../docs/storage/README.md#4-writes-and-deletes): a put never overwrites a key (`If-None-Match: *`, so a second put is `BlobError::AlreadyExists`), a missing key is `NotFound`, and deleting a missing key counts as deleted. Database behaviour is never faked ([database.md](database.md)).

The port below is the shape of `domain::ports::BlobStore` cut to three methods, with `Vec<u8>` for its `Bytes` bodies (the checker doesn't build `bytes`) and `String` for its `ObjectKey`. The fake is an `#[async_trait]` impl like the real one; each method takes the `std::sync::Mutex` for one map operation and never holds it across an `.await`, which is what [`await_holding_lock`](https://clippy.rust-lang.org/rust-1.99.0/index.html#await_holding_lock) checks and why a `std` mutex is right here ([Tokio, shared state](https://tokio.rs/tokio/tutorial/shared-state)). The error is `BlobError`, an exact copy of the canonical one in [rust-errors, mapping.md](../rust-errors/mapping.md), with its `BoxError`.

```rust
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

/// `domain::ports::BlobStore`, cut to three methods (backend domain 9, object storage).
#[mockall::automock]
#[async_trait::async_trait]
trait BlobStore: Send + Sync {
    /// `PutObject` with `If-None-Match: *`: an existing key is `AlreadyExists`.
    async fn put(&self, key: &str, body: Vec<u8>) -> Result<(), BlobError>;
    /// `GetObject`, buffered.
    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, BlobError>;
    /// `DeleteObjects`; returns how many keys count as deleted (a missing key counts).
    async fn delete_keys(&self, keys: &[String]) -> Result<u64, BlobError>;
}

/// In-memory `BlobStore` with the storage §4 rules.
#[derive(Default)]
struct FakeBlobs {
    objects: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl FakeBlobs {
    fn objects(&self) -> MutexGuard<'_, BTreeMap<String, Vec<u8>>> {
        self.objects.lock().expect("fake lock")
    }
}

#[async_trait::async_trait]
impl BlobStore for FakeBlobs {
    async fn put(&self, key: &str, body: Vec<u8>) -> Result<(), BlobError> {
        let mut objects = self.objects();
        if objects.contains_key(key) {
            return Err(BlobError::AlreadyExists);
        }
        objects.insert(key.to_owned(), body);
        Ok(())
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, BlobError> {
        self.objects().get(key).cloned().ok_or(BlobError::NotFound)
    }

    async fn delete_keys(&self, keys: &[String]) -> Result<u64, BlobError> {
        let mut objects = self.objects();
        for key in keys {
            objects.remove(key);
        }
        Ok(keys.len() as u64)
    }
}

/// Code under test: through the port, as `api` holds it.
async fn store_once(blobs: &dyn BlobStore, key: &str, body: Vec<u8>) -> &'static str {
    match blobs.put(key, body).await {
        Ok(()) => "stored",
        // Keys are never overwritten, so an existing key is a bug (rust-errors, mapping.md).
        Err(BlobError::AlreadyExists) => "exists",
        Err(_) => "failed",
    }
}

// copy of: rust-errors/mapping.md BoxError
/// A boxed error from a library, kept as the source of the log chain (`domain::ports`).
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
// end: BoxError

// copy of: rust-errors/mapping.md BlobError
/// The object store's error ([storage 4]).
#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    /// No object under that key.
    #[error("object not found")]
    NotFound,
    /// `If-None-Match: *` refused: the key exists (`412`). Keys are never overwritten.
    #[error("object already exists")]
    AlreadyExists,
    /// `RustFS` can't be reached or timed out.
    #[error("storage unavailable")]
    Unavailable(#[source] BoxError),
    /// `RustFS` refused the call (a policy, a bad key).
    #[error("storage refused {operation}")]
    Rejected {
        /// The S3 operation.
        operation: &'static str,
        /// The SDK error.
        #[source]
        source: BoxError,
    },
}
// end: BlobError

const KEY: &str = "gardens/01JAX3K9T6R2B8V0QZ4M7D1E5F/scans/01JAX3M2W7C5N9P4R8T6V0X2Y4/01JAX3N8D3F7H1K5M9Q3S7V1W5.webp";

#[tokio::test]
async fn the_fake_keeps_the_storage_rules() {
    let blobs = FakeBlobs::default();
    assert_eq!(store_once(&blobs, KEY, b"webp".to_vec()).await, "stored");
    let again = blobs.put(KEY, b"other".to_vec()).await;
    assert!(
        matches!(again, Err(BlobError::AlreadyExists)),
        "a second put never overwrites, got {again:?}"
    );
    assert_eq!(blobs.get_bytes(KEY).await.unwrap(), b"webp");
    assert_eq!(blobs.delete_keys(&[KEY.to_owned()]).await.unwrap(), 1);
    assert_eq!(
        blobs.delete_keys(&[KEY.to_owned()]).await.unwrap(),
        1,
        "a missing key counts as deleted"
    );
    assert!(matches!(
        blobs.get_bytes(KEY).await,
        Err(BlobError::NotFound)
    ));
}

#[tokio::test]
async fn the_same_port_is_mocked_for_one_answer() {
    let mut blobs = MockBlobStore::new();
    blobs
        .expect_put()
        .withf(|key, _| key == KEY)
        .times(1)
        .returning(|_, _| Err(BlobError::AlreadyExists));
    assert_eq!(store_once(&blobs, KEY, Vec::new()).await, "exists");
}
```

The key follows the layout in [storage 2](../../../docs/storage/README.md#2-bucket-keys-and-access). A fake answers state questions; a mock answers "was this called, with what, how often". When both would do, prefer the fake: it survives a change in how often the code calls the port.
