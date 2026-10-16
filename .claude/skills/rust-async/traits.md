# Async functions in traits, `dyn` and async closures

Part of [rust-async](SKILL.md). Rust 1.99, edition 2024, `async-trait` 0.1.92, `mockall` 0.15.0. Each claim about the compiler is backed by a block that compiles, or by a `compile_fail` block that the checker confirms fails.

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

The future an `async fn` in a trait returns is an anonymous type, and the trait gives callers no way to require it to be `Send` (that is what return type notation would do). A generic caller therefore can't hand it to `tokio::spawn`, which needs `Send + 'static` ([tokio::spawn](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn.html)). The compiler rejects it and suggests `-> impl Future + Send`, which fixes generic callers but not `dyn` (section 3):

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

Architecture §2 puts "a trait at every external boundary", and §3 puts them all in `domain` ([design principles](../../../docs/architecture/README.md#design-principles), [Rust crates](../../../docs/architecture/README.md#rust-crates)). The backend design names them the **ports**, in `domain::ports` ([backend domain 9](../../../docs/backend/domain.md#9-boundary-traits)), and fixes how their async methods are written ([backend domain 10, dyn dispatch and async methods](../../../docs/backend/domain.md#dyn-dispatch-and-async-methods)). Rules:

1. **Every port is `Send + Sync`** and is held as `Arc<dyn Port>`: `AppState` holds only ports, so `State<Arc<AppState>>` is one concrete type and tests swap in mocks ([backend README 7, dispatch](../../../docs/backend/README.md#7-rust-conventions); [backend api 6](../../../docs/backend/api.md#6-application-state-wiring-and-the-seams)). `Arc<dyn Port>` means `Arc<dyn Port + 'static>` by the default object lifetime, so no port adds `'static` as a supertrait ([Reference, default trait object lifetimes](https://doc.rust-lang.org/reference/lifetime-elision.html#default-trait-object-lifetimes)).
2. **A port's async methods use `#[async_trait]`** (`async-trait` 0.1.92), on the trait and on every `impl`. The macro rewrites `async fn m(&self, …) -> T` into `fn m<'life0, …, 'async_trait>(&'life0 self, …) -> Pin<Box<dyn Future<Output = T> + Send + 'async_trait>>`: a nameable return type, so the trait stays dyn compatible, and a `Send` future, so `tokio::spawn` accepts it. Every borrowed argument gets its own lifetime, so no signature spells one ([async-trait 0.1.92](https://docs.rs/async-trait/0.1.92/async_trait/); [backend domain 10](../../../docs/backend/domain.md#dyn-dispatch-and-async-methods)). The cost is one allocation per call, nothing next to a database, network or model call.
3. **Not native `async fn` or `-> impl Future` in a port.** Neither is dyn compatible on Rust 1.99, so `Arc<dyn Port>` fails with `E0038` ([Reference, dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility); the two `compile_fail,E0038` blocks below).
4. **Not a hand-written `BoxFuture<'a, T>` in a port.** It has the same runtime shape, but `mockall` 0.15 can't mock a method whose boxed future borrows a named lifetime: `#[automock]` fails with `E0521`, "borrowed data escapes outside of method" (the `compile_fail,E0521` block below; [backend domain 10](../../../docs/backend/domain.md#dyn-dispatch-and-async-methods)). With `#[async_trait]` the same trait mocks cleanly: put `#[automock]` above `#[async_trait]` ([mockall 0.15.0, async traits](https://docs.rs/mockall/0.15.0/mockall/index.html#async-traits)). How tests use the mocks is in [rust-testing, mocks](../rust-testing/mocks.md).
5. **Synchronous ports stay plain methods**: `Clock`, `IdGen`, `UrlSigner`, `SpeciesPhotos`, `WebhookVerifier`, `SpotSegmenter` do microseconds of CPU work or run on a blocking thread ([backend domain 10](../../../docs/backend/domain.md#dyn-dispatch-and-async-methods)).
6. **A port that streams returns `BoxStream<'a, …>` from a plain method.** `ChatModel::stream<'a>(&'a self, request: &'a ChatRequest) -> BoxStream<'a, Result<ChatChunk, ModelError>>` ties the stream to the model and the borrowed request, so the adapter streams without cloning the request; `complete` beside it is an `#[async_trait]` method ([backend domain 9, models](../../../docs/backend/domain.md#models); [named lifetimes](../../../docs/backend/domain.md#named-lifetimes)). The SSE body must be `'static`, so the caller moves the `Arc<dyn ChatModel>` and the owned request into a stream generator and borrows them inside it (the `the_chat_stream_owns_what_it_borrows` test). mockall can't mock this method either, so `domain`'s `mock` feature covers every port but `ChatModel`, and tests use the hand-written `ScriptedChat` ([backend domain 11](../../../docs/backend/domain.md#11-testing)).
7. **Crate-internal traits keep `impl Future` or `BoxFuture`.** The agent's typed `Tool` returns `impl Future<Output = …> + Send` and has associated constants, so it is used with static dispatch only; the registry erases each tool once behind `DynTool`, whose `call<'a>` returns `BoxFuture<'a, …>` because a boxed future needs a named bound ([backend agent 12, typed tools, erased once](../../../docs/backend/agent.md#typed-tools-erased-once); [agent 15](../../../docs/backend/agent.md#15-concurrency-and-lifetimes)). Nothing mocks them: tests run real tools against mocked ports.

| The trait is… | Form |
|---|---|
| A port in `domain::ports` (held as `Arc<dyn Port>`, mocked in tests) | `#[async_trait]` with `async fn`; `Send + Sync` |
| A port method that returns a stream | A plain `fn m<'a>(&'a self, …) -> BoxStream<'a, …>` in the same `#[async_trait]` trait |
| A synchronous port | Plain methods |
| Crate-internal, static dispatch only (`Tool`) | `fn m(&self, …) -> impl Future<Output = T> + Send` |
| Crate-internal, erased for a registry (`DynTool`) | `fn m<'a>(&'a self, …) -> BoxFuture<'a, T>` |

A port with an `#[async_trait]` method and a streaming method, its mock, a `dyn` call inside a spawned task, and the agent's erased tool:

```rust
use std::future::Future;
use std::sync::Arc;

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, Stream, StreamExt};

/// Stands in for each port's error enum (`domain::ports`, backend domain 6).
#[derive(Debug, thiserror::Error)]
#[error("the boundary call failed")]
pub struct CallFailed;

/// A port: `#[async_trait]`, `Send + Sync`, held as `Arc<dyn SpeciesIdentifier>`. Both
/// borrowed arguments get their own lifetime from the macro.
#[mockall::automock]
#[async_trait::async_trait]
pub trait SpeciesIdentifier: Send + Sync {
    async fn identify(&self, garden: &str, image: &[u8]) -> Result<String, CallFailed>;
}

pub struct ChatRequest {
    pub messages: Vec<String>,
}

/// The chat port: a plain method returning a stream tied to `self` and the request, and an
/// `#[async_trait]` method for whole answers (backend domain 9, models).
#[async_trait::async_trait]
pub trait ChatModel: Send + Sync {
    fn stream<'a>(&'a self, request: &'a ChatRequest) -> BoxStream<'a, Result<String, CallFailed>>;

    async fn complete(&self, request: &ChatRequest) -> Result<String, CallFailed>;
}

/// The hand-written fake tests use for `ChatModel`, which mockall can't mock.
pub struct ScriptedChat(pub Vec<String>);

#[async_trait::async_trait]
impl ChatModel for ScriptedChat {
    fn stream<'a>(&'a self, request: &'a ChatRequest) -> BoxStream<'a, Result<String, CallFailed>> {
        // Borrows the script and the request; nothing is cloned until an item is produced.
        stream::iter(self.0.iter().chain(&request.messages))
            .map(|text| Ok(text.clone()))
            .boxed()
    }

    async fn complete(&self, request: &ChatRequest) -> Result<String, CallFailed> {
        Ok(format!("{} turns", request.messages.len()))
    }
}

/// The SSE body must be `'static`: own the model and the request, borrow them inside.
pub fn answer_stream(
    model: Arc<dyn ChatModel>,
    request: ChatRequest,
) -> impl Stream<Item = Result<String, CallFailed>> + Send + 'static {
    async_stream::stream! {
        let mut chunks = model.stream(&request);
        while let Some(chunk) = chunks.next().await {
            yield chunk;
        }
    }
}

/// A crate-internal typed tool: static dispatch, so `impl Future` and an associated const.
pub trait Tool: Send + Sync + 'static {
    const NAME: &'static str;

    /// Under edition 2024 the future captures `self` and `ctx`.
    fn run(
        &self,
        ctx: &str,
        args: String,
    ) -> impl Future<Output = Result<String, CallFailed>> + Send;
}

/// The dyn-compatible face of a tool, for the registry. One named `'a` for the tool and the
/// turn's context, because the boxed future borrows both and elision can't name it.
pub trait DynTool: Send + Sync {
    fn name(&self) -> &'static str;

    fn call<'a>(
        &'a self,
        ctx: &'a str,
        arguments: String,
    ) -> BoxFuture<'a, Result<String, CallFailed>>;
}

impl<T: Tool> DynTool for T {
    fn name(&self) -> &'static str {
        T::NAME
    }

    fn call<'a>(
        &'a self,
        ctx: &'a str,
        arguments: String,
    ) -> BoxFuture<'a, Result<String, CallFailed>> {
        Box::pin(self.run(ctx, arguments))
    }
}

pub struct Echo;

impl Tool for Echo {
    const NAME: &'static str = "echo";

    async fn run(&self, ctx: &str, args: String) -> Result<String, CallFailed> {
        tokio::task::yield_now().await;
        Ok(format!("{ctx}: {args}"))
    }
}

#[tokio::test]
async fn a_mocked_port_is_called_through_dyn_in_a_spawned_task() {
    let mut mock = MockSpeciesIdentifier::new();
    mock.expect_identify()
        .withf(|garden, image| garden == "g1" && image.len() == 3)
        .returning(|_, _| Ok("tomato".to_owned()));
    let port: Arc<dyn SpeciesIdentifier> = Arc::new(mock);
    let found = tokio::spawn(async move { port.identify("g1", &[1, 2, 3]).await })
        .await
        .expect("no panic");
    assert_eq!(found.ok().as_deref(), Some("tomato"));
}

#[tokio::test]
async fn the_chat_stream_owns_what_it_borrows() {
    fn assert_static_send<S: Send + 'static>(_: &S) {}
    let model: Arc<dyn ChatModel> = Arc::new(ScriptedChat(vec!["Hello".to_owned()]));
    let request = ChatRequest {
        messages: vec!["How is my tomato?".to_owned()],
    };
    assert_eq!(
        model.complete(&request).await.ok().as_deref(),
        Some("1 turns")
    );
    let body = answer_stream(Arc::clone(&model), request);
    assert_static_send(&body);
    let chunks: Vec<String> = body.map(|chunk| chunk.expect("scripted")).collect().await;
    assert_eq!(chunks, ["Hello", "How is my tomato?"]);
}

#[tokio::test]
async fn an_erased_tool_runs_from_the_registry() {
    let registry: Vec<Box<dyn DynTool>> = vec![Box::new(Echo)];
    let tool = registry
        .iter()
        .find(|t| t.name() == "echo")
        .expect("registered");
    let turn = "turn 1".to_owned();
    let out = tool.call(&turn, "hi".to_owned()).await;
    assert_eq!(out.ok().as_deref(), Some("turn 1: hi"));
}
```

A hand-written `BoxFuture` that borrows a named lifetime can't be mocked:

```rust,compile_fail,E0521
use futures::future::BoxFuture;

#[mockall::automock]
pub trait Store: Send + Sync {
    fn load<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<u32, String>>;
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

Where a trait does return `impl Future + Send` (the crate-internal `Tool`), the promise is enforced on each implementation, so a non-`Send` body is caught where it is written:

```rust,compile_fail
use std::future::Future;
use std::rc::Rc;

pub trait Tool: Send + Sync {
    fn run(&self, args: &str) -> impl Future<Output = Option<String>> + Send;
}

pub struct Cached;

impl Tool for Cached {
    async fn run(&self, args: &str) -> Option<String> {
        let shared = Rc::new(args.to_owned()); // Rc is !Send
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
