# Async functions in traits, `dyn` and async closures

Part of [rust-async](SKILL.md). Rust 1.99, edition 2024. Each claim about the compiler is backed by a block that compiles, or by a `compile_fail` block that the checker confirms fails.

## Contents

1. [What is stable in Rust 1.99](#1-what-is-stable-in-rust-199)
2. [The `Send` problem](#2-the-send-problem)
3. [What the project does with its boundary traits](#3-what-the-project-does-with-its-boundary-traits)
4. [Async closures](#4-async-closures)

## 1. What is stable in Rust 1.99

| Feature | Status | Source |
|---|---|---|
| `async fn` in traits, and `-> impl Trait` in trait methods (RPITIT) | Stable since **1.75** | [Release notes 1.75.0, Language](https://doc.rust-lang.org/releases.html#1.75.0-Language); [Reference: RPITIT](https://doc.rust-lang.org/reference/types/impl-trait.html#return-position-impl-trait-in-traits-and-trait-implementations) |
| A trait with an `async fn` or an `-> impl Trait` method used as `dyn Trait` | **Not allowed**: such a method is not dispatchable, so the trait is not dyn compatible (`E0038`) | [Reference: Dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility) |
| Async closures (`async \|\| …`) and the `AsyncFn`, `AsyncFnMut`, `AsyncFnOnce` traits | Stable since **1.85**; the traits are in the prelude of every edition | [Release notes 1.85.0, Language](https://doc.rust-lang.org/releases.html#1.85.0-Language) |
| `dyn AsyncFn…` | **Not allowed**: "The `AsyncFn`, `AsyncFnMut`, and `AsyncFnOnce` traits are not dyn-compatible" | [Reference](https://doc.rust-lang.org/reference/items/traits.html#r-items.traits.dyn-compatible.async-traits) |
| Return type notation (`T: Trait<method(..): Send>`) | **Unstable**: error `E0658`, "return type notation is experimental". It doesn't appear in the release notes up to 1.99 | [Unstable Book: return_type_notation](https://doc.rust-lang.org/nightly/unstable-book/language-features/return-type-notation.html); the `compile_fail,E0658` block below |

An `async fn` in a trait is sugar for a method returning `impl Future<Output = …>` that captures every lifetime of the signature ([Reference: async functions](https://doc.rust-lang.org/reference/items/functions.html#r-items.fn.async.desugar-brief), [lifetime capture](https://doc.rust-lang.org/reference/items/functions.html#r-items.fn.async.lifetime-capture)). An implementation of a method declared `-> impl Future<Output = T> + Send` may be written as an `async fn` (section 3).

```rust,compile_fail,E0658
pub trait Store {
    async fn load(&self) -> u8;
}

fn spawn_load<S: Store<load(..): Send> + Send + Sync + 'static>(s: S) {
    tokio::spawn(async move { s.load().await });
}
```

## 2. The `Send` problem

The future an `async fn` in a trait returns is an anonymous type, and the trait gives callers no way to require it to be `Send` (that is what return type notation would do). A generic caller therefore can't hand it to `tokio::spawn`, which needs `Send + 'static` ([tokio::spawn](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn.html)). The compiler rejects it and suggests the fix used in section 3:

```rust,compile_fail
pub trait VisionModel {
    async fn analyse(&self, image: Vec<u8>) -> String;
}

fn spawn_analysis<V: VisionModel + Send + Sync + 'static>(model: std::sync::Arc<V>) {
    tokio::spawn(async move {
        let _ = model.analyse(Vec::new()).await;
    });
}
```

The error is "future cannot be sent between threads safely" (no error code), with "help: `Send` can be made part of the associated future's guarantees for all implementations of `VisionModel::analyse`" and the rewrite to `fn analyse(&self, image: Vec<u8>) -> impl std::future::Future<Output = String> + Send`. For the same reason rustc's `async_fn_in_trait` lint ("use of `async fn` in public traits is discouraged as auto trait bounds cannot be specified") warns on every `pub` trait with an `async fn`, such as every trait in `domain`, and the project allows no warnings (both checked with rustc 1.99.1; the error is the `compile_fail` block above). [rustc lints: async_fn_in_trait](https://doc.rust-lang.org/rustc/lints/listing/warn-by-default.html#async-fn-in-trait)

## 3. What the project does with its boundary traits

Architecture §2 puts "a trait at every external boundary" (`VisionModel`, `SpeciesIdentifier`, `TextClassifier`, `TextEmbedder`, `ChatModel`, `KnowledgeStore`, `WeatherProvider`, `SoilProvider`, `BlobStore`, `Store`, `MemoryStore`, `Notifier`), and §3 puts them all in `domain` ([design principles](../../../docs/architecture/README.md#design-principles), [Rust crates](../../../docs/architecture/README.md#rust-crates)). Rules:

1. **Every boundary trait is `Send + Sync + 'static`** (`'static` where it's held in `Arc` and shared with spawned tasks), and **every async method promises a `Send` future**. There is no single-threaded caller in the backend.
2. **Never write `async fn` in a `domain` trait declaration.** Write one of the two forms below. Implementations may still use `async fn`; the compiler then checks the body against the trait's `Send` promise.
3. **Pick the form per trait:**

| The trait is… | Form | Why |
|---|---|---|
| Held in `AppState` as `Arc<dyn Trait>` (so `State<Arc<AppState>>` stays one concrete type and tests can swap a fake), or chosen at run time (the worker's `VisionModel` by `--caps`; `TextClassifier`, which falls back from Jev to the LLM API's cheapest model, [architecture 10](../../../docs/architecture/README.md#10-models-and-gpu-placement)). `ChatModel` is the first kind: it has no fallback (chat and the plan writer use the LLM API's strongest model, architecture 10), but the agent core holds it in `AppState` | `fn m(&self, …) -> BoxFuture<'_, Result<T, E>>` (`futures::future::BoxFuture`) | `dyn` needs a nameable return type; one allocation per call is nothing next to a network or model call |
| Only ever used through generics (`fn run<K: KnowledgeStore>(…)`) | `fn m(&self, …) -> impl Future<Output = Result<T, E>> + Send` | No allocation; not usable as `dyn` |

When in doubt, use the boxed form: it is the only one that works both ways (`Arc<dyn Trait>` and generics). *This skill's choice*, from the two `compile_fail,E0038` blocks below.

**`BlobStore` uses the `impl Future` form** (the project's decision, here and in [rust-testing, mocks](../rust-testing/mocks.md)): it has one production implementation, the `blob` crate's RustFS client, which `AppState` holds as a concrete type ([architecture 3, Rust crates](../../../docs/architecture/README.md#rust-crates)), and tests either write an in-memory fake or use mockall, which mocks `impl Future` methods ([mockall: impl Future](https://docs.rs/mockall/0.15.0/mockall/index.html#impl-future)). How the tests mock these traits is in [rust-testing](../rust-testing/SKILL.md).

Both forms compile, spawn, and run:

```rust
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use futures::future::BoxFuture;
use tokio::sync::RwLock;

/// Stands in for each boundary's own error type.
#[derive(Debug, thiserror::Error)]
#[error("the boundary call failed")]
pub struct CallFailed;

/// Static dispatch: the trait promises a `Send` future, so generic callers can spawn it.
pub trait BlobStore: Send + Sync + 'static {
    fn get(&self, key: &str) -> impl Future<Output = Result<Vec<u8>, CallFailed>> + Send;
}

#[derive(Default)]
pub struct MemoryBlobs(RwLock<HashMap<String, Vec<u8>>>);

impl BlobStore for MemoryBlobs {
    // An impl may still write `async fn`; the compiler checks the future is `Send`.
    async fn get(&self, key: &str) -> Result<Vec<u8>, CallFailed> {
        self.0.read().await.get(key).cloned().ok_or(CallFailed)
    }
}

pub fn spawn_fetch<B: BlobStore>(blobs: Arc<B>, key: String) -> tokio::task::JoinHandle<bool> {
    tokio::spawn(async move { blobs.get(&key).await.is_ok() })
}

/// Dynamic dispatch: a boxed `Send` future, so the trait works as `dyn VisionModel`.
pub trait VisionModel: Send + Sync {
    fn name(&self) -> &'static str;
    fn analyse<'a>(&'a self, image: &'a [u8]) -> BoxFuture<'a, Result<u32, CallFailed>>;
}

pub struct FallbackVision;

impl VisionModel for FallbackVision {
    fn name(&self) -> &'static str {
        "fallback"
    }
    fn analyse<'a>(&'a self, image: &'a [u8]) -> BoxFuture<'a, Result<u32, CallFailed>> {
        Box::pin(async move {
            tokio::task::yield_now().await; // stands in for the LLM API call
            u32::try_from(image.len()).map_err(|_| CallFailed)
        })
    }
}

#[tokio::test]
async fn static_dispatch_spawns() {
    let blobs = Arc::new(MemoryBlobs::default());
    blobs.0.write().await.insert("a".to_owned(), vec![1]);
    let found = spawn_fetch(blobs, "a".to_owned()).await.expect("no panic");
    assert!(found);
}

#[tokio::test]
async fn dyn_dispatch_spawns() {
    let model: Arc<dyn VisionModel> = Arc::new(FallbackVision);
    let out = tokio::spawn(async move { model.analyse(&[1, 2, 3, 4]).await.ok() })
        .await
        .expect("no panic");
    assert_eq!(out, Some(4));
}
```

The `impl Future` form can't be used as `dyn`:

```rust,compile_fail,E0038
use std::future::Future;

pub trait BlobStore: Send + Sync {
    fn get(&self, key: &str) -> impl Future<Output = Vec<u8>> + Send;
}

pub struct AppState {
    pub blobs: std::sync::Arc<dyn BlobStore>,
}
```

Nor can a trait with `async fn`:

```rust,compile_fail,E0038
pub trait ChatModel: Send + Sync {
    async fn reply(&self, prompt: &str) -> String;
}

pub struct AppState {
    pub chat: std::sync::Arc<dyn ChatModel>,
}
```

The trait's `Send` promise is enforced on each implementation, so a non-`Send` body is caught where it is written:

```rust,compile_fail
use std::future::Future;
use std::rc::Rc;

pub trait KnowledgeStore: Send + Sync {
    fn lookup(&self, slug: &str) -> impl Future<Output = Option<String>> + Send;
}

pub struct Cached;

impl KnowledgeStore for Cached {
    async fn lookup(&self, slug: &str) -> Option<String> {
        let shared = Rc::new(slug.to_owned()); // Rc is !Send
        tokio::task::yield_now().await;
        Some((*shared).clone())
    }
}
```

## 4. Async closures

Async closures are stable (1.85), and an `async || { … }` closure can borrow from its surroundings ([release notes 1.85.0](https://doc.rust-lang.org/releases.html#1.85.0-Language)). But an `AsyncFn*` **bound** can't say that the returned future is `Send` (that would again need return type notation). In rustc 1.99.1 that matters only when the closure **borrows** from the enclosing function: a helper bounded by `AsyncFnMut` runs inside `tokio::spawn` with a closure that captures nothing, or one that owns its state (`async move`):

```rust
pub async fn retry<T, E>(attempts: u32, mut op: impl AsyncFnMut() -> Result<T, E>) -> Result<T, E> {
    let mut n = 1;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if n >= attempts => return Err(e),
            Err(_) => n += 1,
        }
    }
}

async fn ping() -> Result<u8, ()> {
    tokio::task::yield_now().await;
    Ok(1)
}

#[tokio::test]
async fn a_closure_that_borrows_nothing_spawns() {
    // Captures nothing.
    let out = tokio::spawn(async { retry(3, async || ping().await).await }).await;
    assert_eq!(out.expect("no panic"), Ok(1));

    // Owns its state: `calls` is moved into the closure.
    let out = tokio::spawn(async {
        let mut calls = 0_u8;
        retry(3, async move || {
            calls += 1;
            if calls < 2 { Err(()) } else { Ok(calls) }
        })
        .await
    })
    .await;
    assert_eq!(out.expect("no panic"), Ok(2));
}
```

The same helper with a closure that borrows a local of the enclosing `async fn` (`calls` by `&mut`) doesn't compile once the future is spawned:

```rust,compile_fail
async fn retry<T, E>(attempts: u32, mut op: impl AsyncFnMut() -> Result<T, E>) -> Result<T, E> {
    let mut n = 1;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if n >= attempts => return Err(e),
            Err(_) => n += 1,
        }
    }
}

async fn fetch() -> Result<u8, ()> {
    let mut calls = 0;
    retry(3, async || {
        calls += 1;
        if calls < 2 { Err(()) } else { Ok(calls) }
    })
    .await
}

fn start() {
    tokio::spawn(fetch());
}
```

rustc 1.99.1 reports one error, "higher-ranked lifetime error", which has no error code (so the fence names none), even though the closure itself is `Send`. A shared borrow (`async || if limit > 1 { … }` reading a local) fails the same way, with "implementation of `Send` is not general enough", also without a code. For helpers that end up in spawned tasks and take closures that borrow (retries around SurrealDB conflicts, [database 6](../../../docs/database/README.md#6-rust-sdk); HTTP retries in the workers), bound by `FnMut() -> Fut` with `Fut: Future`, which works with borrowing closures:

```rust
use std::future::Future;
use std::time::Duration;

pub async fn retry<T, E, F, Fut>(attempts: u32, mut op: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut n = 1;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if n >= attempts => return Err(e),
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(10 << n)).await;
                n += 1;
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn retry_inside_a_spawned_task() {
    let out = tokio::spawn(async {
        let calls = std::sync::atomic::AtomicU32::new(0);
        retry(3, || async {
            let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n < 2 { Err(n) } else { Ok(n) }
        })
        .await
    })
    .await
    .expect("no panic");
    assert_eq!(out, Ok(2));
}
```

The back-off here is only a placeholder; the conflict retry's real schedule (5 attempts, 10–50 ms × 2^attempt with jitter) is in [database 6](../../../docs/database/README.md#6-rust-sdk) and [rust-surrealdb](../rust-surrealdb/SKILL.md).
