# Cancellation: `select!`, tasks and `CancellationToken`

Part of [rust-async](SKILL.md). Versions: tokio 1.53.3, tokio-util 0.7.20. Every behaviour below is proved by the test block under it.

## Contents

1. [`select!`](#1-select)
2. [Tasks: `JoinHandle`, `abort`, `JoinSet`](#2-tasks-joinhandle-abort-joinset)
3. [`CancellationToken`](#3-cancellationtoken)

## 1. `select!`

**What it does.** `select!` runs its branches concurrently on the current task (not in parallel), returns when the first one completes with a value that matches its pattern, and **drops every other branch's future**. A branch whose pattern doesn't match (for example `Some(m) = rx.recv()` getting `None`) is disabled for the rest of that call; with every branch disabled and no `else`, it panics. [select! docs](https://docs.rs/tokio/1.53.3/tokio/macro.select.html), [Panics](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#panics)

**Cancellation safety** means "it must be a no-op to drop that future and recreate it". In a `loop { select! { … } }` every branch that loses is dropped and created again on the next turn, so every branch must be cancel safe, or the loss must not matter (for example, the process is shutting down). The docs' lists, quoted ("the lists in this section are not exhaustive"): [Cancellation safety](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#cancellation-safety)

| Cancel safe | Not cancel safe: can lose data | Not cancel safe: lose their place in a fair queue |
|---|---|---|
| `mpsc::Receiver::recv`, `mpsc::UnboundedReceiver::recv`, `broadcast::Receiver::recv`, `watch::Receiver::changed`, `TcpListener::accept`, `UnixListener::accept`, `signal::unix::Signal::recv`, `AsyncReadExt::read`, `AsyncReadExt::read_buf`, `AsyncWriteExt::write`, `AsyncWriteExt::write_buf`, `tokio_stream::StreamExt::next`, `futures::stream::StreamExt::next` | `AsyncReadExt::read_exact`, `AsyncReadExt::read_to_end`, `AsyncReadExt::read_to_string`, `AsyncWriteExt::write_all` | `Mutex::lock`, `RwLock::read`, `RwLock::write`, `Semaphore::acquire`, `Notify::notified` |

Methods with their own "Cancel safety" section that matter here: `CancellationToken::cancelled` and `cancelled_owned` are cancel safe ([docs](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.cancelled)); `JoinSet::join_next` is cancel safe ([docs](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinSet.html#method.join_next)); awaiting `&mut JoinHandle` and `&mut oneshot::Receiver` is cancel safe ([JoinHandle](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinHandle.html#cancel-safety), [oneshot::Receiver](https://docs.rs/tokio/1.53.3/tokio/sync/oneshot/struct.Receiver.html#cancel-safety)); `Interval::tick` is cancel safe ([docs](https://docs.rs/tokio/1.53.3/tokio/time/struct.Interval.html#method.tick)). `mpsc::Sender::send` is not: if another branch wins, the message is dropped and lost; use `reserve` and send through the `Permit` instead ([docs](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Sender.html#method.send)).

Rules:

- Put only cancel-safe futures in a `select!` inside a loop. To make a non-cancel-safe operation safe, move it out of the `select!`: run it to completion inside the branch body, or in its own task fed by a channel. [select!: Cancellation safety](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#cancellation-safety); the test `read_exact_in_a_select_loop_loses_bytes` below
- A future that must survive several loop turns (a deadline, a long request) is created **once**, outside the loop, pinned (`tokio::pin!` or `std::pin::pin!`), and polled as `&mut fut`. A `sleep(…)` written inside the `select!` starts again on every turn. [Examples: "Using the same future in multiple `select!`"](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#examples)
- Without `biased;`, `select!` picks the branch to poll first at random. Write `biased;` when order matters, such as "always see shutdown before taking more work"; then the branches are polled top to bottom, and you are responsible for fairness (put the shutdown branch first so a busy stream can't starve it). [Fairness](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#fairness)
- Don't guard a branch with a check like `if !sleep.is_elapsed()`: the check and the poll race. [Avoid racy `if` preconditions](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#avoid-racy-if-preconditions)
- `select!` is concurrency on one task: a branch that blocks the thread stops every other branch. [Runtime characteristics](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#runtime-characteristics)

```rust
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep, sleep_until};

#[tokio::test(start_paused = true)]
async fn recv_in_a_select_loop_loses_nothing() {
    let (tx, mut rx) = mpsc::channel(8);
    tokio::spawn(async move {
        for i in 0..3 {
            sleep(Duration::from_millis(1500)).await;
            tx.send(i).await.expect("receiver is alive");
        }
    });
    let mut got = Vec::new();
    let mut ticks = 0;
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Some(m) => got.push(m),
                None => break,
            },
            () = sleep(Duration::from_secs(1)) => ticks += 1,
        }
    }
    assert_eq!(got, [0, 1, 2]);
    assert!(ticks > 0, "the sleep branch won several times");
}

#[tokio::test(start_paused = true)]
async fn read_exact_in_a_select_loop_loses_bytes() {
    let (mut writer, mut reader) = tokio::io::duplex(64);
    tokio::spawn(async move {
        writer.write_all(&[1, 2]).await.expect("open");
        sleep(Duration::from_secs(5)).await;
        writer.write_all(&[3, 4]).await.expect("open");
        // dropping the writer ends the stream
    });
    let mut buf = [0u8; 4];
    let result = loop {
        tokio::select! {
            r = reader.read_exact(&mut buf) => break r,
            () = sleep(Duration::from_secs(1)) => {} // drops the half-done read_exact
        }
    };
    // Four bytes were sent, but the dropped read_exact took [1, 2] with it.
    let err = result.expect_err("the stream ends before 4 more bytes arrive");
    assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
}

#[tokio::test(start_paused = true)]
async fn a_pinned_deadline_spans_loop_iterations() {
    let (tx, mut rx) = mpsc::channel::<u32>(8);
    let start = Instant::now();
    let deadline = sleep_until(start + Duration::from_secs(35));
    tokio::pin!(deadline);
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(10)).await;
            if tx.send(1).await.is_err() {
                break;
            }
        }
    });
    let mut received = 0;
    loop {
        tokio::select! {
            () = &mut deadline => break,
            Some(n) = rx.recv() => received += n,
        }
    }
    // a fresh `sleep(35 s)` in each iteration would never fire: the 10 s messages reset it
    assert_eq!(start.elapsed(), Duration::from_secs(35));
    assert_eq!(received, 3); // at 10, 20 and 30 s
}

#[tokio::test]
async fn biased_polls_branches_in_order() {
    let (tx, mut rx) = mpsc::channel(8);
    tx.send(1).await.expect("open");
    let token = tokio_util::sync::CancellationToken::new();
    token.cancel();
    // Both branches are ready. With `biased;` the first one listed always wins.
    let winner = tokio::select! {
        biased;
        () = token.cancelled() => "shutdown",
        _ = rx.recv() => "message",
    };
    assert_eq!(winner, "shutdown");
}
```

## 2. Tasks: `JoinHandle`, `abort`, `JoinSet`

- **A task starts running when it is spawned**, whether or not anyone awaits its handle. **Dropping a `JoinHandle` detaches the task**: it keeps running and its output is lost. So a dropped handle is never a way to stop work. The project relies on this for work that must never be cancelled: every RustFS delete runs in a task owned by `blob::DeleteQueue`, so a client hanging up (axum drops the request's future) or a cascade being dropped never interrupts one ([backend blob 6](../../../docs/backend/blob.md#6-deletes-that-are-never-cancelled); [storage 4](../../../docs/storage/README.md#4-writes-and-deletes)). [JoinHandle](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinHandle.html)
- **`abort()` only schedules cancellation.** The task stops at its next `.await` (an idle task is stopped without running again), its locals are dropped, and awaiting the handle gives a `JoinError` with `is_cancelled()`. A task that reaches its end without yielding may still finish normally. Await the handle to know it has stopped. [task: Cancellation](https://docs.rs/tokio/1.53.3/tokio/task/index.html#cancellation), [abort](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinHandle.html#method.abort)
- **`spawn_blocking` work can't be aborted** once it has started; `abort` has no effect on it. Cancel it cooperatively (a token it checks between steps). [abort](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinHandle.html#method.abort), [blocking.md](blocking.md)
- **A panic in a task doesn't crash the process.** Tokio catches it and the handle (or `join_next`) returns `Err(e)` with `e.is_panic()`. Handle that case explicitly; don't `unwrap` it away in production code ([rust-errors](../rust-errors/SKILL.md) owns what to do with it). [JoinHandle examples](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinHandle.html), [JoinError::is_panic](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinError.html#method.is_panic)
- **`JoinSet`** holds tasks with the same output type. `join_next` returns them in completion order and `None` when the set is empty. **Dropping a `JoinSet` aborts every task in it**; `shutdown()` aborts them and waits; `join_all` panics if any task failed, so prefer a `join_next` loop. Use it for a fan-out whose results you collect (the perception stages of one request, the fallback worker's parallel calls). [JoinSet](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinSet.html), [join_all](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinSet.html#method.join_all)
- For long-lived background tasks that you only need to wait for at shutdown, use `TaskTracker` instead: a `JoinSet` keeps every finished task's output until `join_next` takes it, while a `TaskTracker` frees a task as soon as it exits, and dropping a `TaskTracker` does not abort anything. [TaskTracker: Comparison to JoinSet](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html#comparison-to-joinset), [shutdown.md](shutdown.md)

```rust
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::sleep;

#[tokio::test(start_paused = true)]
async fn a_panic_surfaces_as_a_join_error() {
    let mut set = JoinSet::new();
    set.spawn(async { panic!("model crashed") });
    set.spawn(async { 7 });
    let mut ok = Vec::new();
    let mut panics = 0;
    while let Some(res) = set.join_next().await {
        match res {
            Ok(v) => ok.push(v),
            Err(e) if e.is_panic() => panics += 1,
            Err(e) => panic!("unexpected join error: {e}"),
        }
    }
    assert_eq!((ok, panics), (vec![7], 1));
    assert!(set.join_next().await.is_none(), "an empty set yields None");
}

#[tokio::test(start_paused = true)]
async fn dropping_a_join_set_aborts_its_tasks() {
    let (alive, gone) = oneshot::channel::<()>();
    let mut set = JoinSet::new();
    set.spawn(async move {
        let _alive = alive; // dropped when the task is dropped
        sleep(Duration::from_secs(3600)).await;
    });
    drop(set);
    assert!(gone.await.is_err(), "the task's locals were dropped");
}

#[tokio::test(start_paused = true)]
async fn dropping_a_join_handle_detaches_the_task() {
    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        sleep(Duration::from_secs(1)).await;
        tx.send("still ran").expect("receiver is alive");
    });
    drop(handle);
    assert_eq!(rx.await.expect("task finished"), "still ran");
}

#[tokio::test(start_paused = true)]
async fn abort_stops_the_task_at_its_next_await() {
    let handle = tokio::spawn(async {
        sleep(Duration::from_secs(3600)).await;
        1
    });
    handle.abort();
    let err = handle.await.expect_err("aborted before it finished");
    assert!(err.is_cancelled());
}
```

## 3. `CancellationToken`

`tokio_util::sync::CancellationToken` (feature `rt` is not needed for it; it is in `tokio_util::sync`). The process has one root token ([shutdown.md](shutdown.md)); each subsystem gets a `child_token()`, so it can be stopped on its own without stopping the rest.

| Operation | Effect | Source |
|---|---|---|
| `clone()` | Another handle to the **same** token: cancelling either cancels both | [Clone](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#impl-Clone-for-CancellationToken) |
| `child_token()` | A new token that is cancelled when this one is. Cancelling the child does **not** cancel the parent (or its siblings). A child of an already-cancelled token starts cancelled | [child_token](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.child_token) |
| `cancel()` | Cancels the token and every token derived from it, and wakes every waiter. Once it returns, all children are cancelled | [cancel](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.cancel) |
| `drop(token)` | **Cancels nothing**, parent or child. Dropping the last handle of a token moves its children to its parent, so they still hear a cancel from further up | Source: `impl Drop for CancellationToken` calls `tree_node::decrease_handle_refcount` (`tokio-util-0.7.20/src/sync/cancellation_token/tree_node.rs`); proved below |
| `drop_guard()` / `drop_guard_ref()` | A guard that **does** cancel the token (and its children) when dropped, unless `disarm()`ed | [drop_guard](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.drop_guard), [DropGuard::disarm](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.DropGuard.html#method.disarm) |
| `is_cancelled()` | A plain check, usable from blocking code | [is_cancelled](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.is_cancelled) |
| `cancelled()` | A future that completes on cancellation (at once if already cancelled). Borrows the token. Cancel safe | [cancelled](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.cancelled) |
| `cancelled_owned()` | The same, but takes the token by value, so the future is `'static`: use it for `with_graceful_shutdown(token.clone().cancelled_owned())` and `tokio::spawn` | [cancelled_owned](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.cancelled_owned) |
| `run_until_cancelled(fut)` | `Some(output)` if `fut` finishes first, `None` if the token is cancelled first, and then `fut` is dropped. On an already-cancelled token it returns `None` without polling `fut`; after that a tie goes to `fut`. Cancel safe only if `fut` is | [run_until_cancelled](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.run_until_cancelled) |

Rules:

- Use a `DropGuard` when "this scope ended, by any path" must stop something, for example a per-job child token that must be cancelled even if the job function returns early with `?`. Otherwise call `cancel()` explicitly. Never rely on dropping a token to stop anything. [drop_guard](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.drop_guard); the tests `dropping_a_token_cancels_nothing` and `a_drop_guard_cancels_when_dropped_unless_disarmed` below
- Cancellation is a request. The task decides where to stop: a `select!` branch on `cancelled()`, `run_until_cancelled`, or `is_cancelled()` between blocking steps. Finish or abandon the current unit of work in a state the system can recover from: a job that stops heartbeating is requeued ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)), and the sweep recovers `api`'s own steps ([contracts 3, scan lifecycle](../../../docs/architecture/contracts.md#scan-lifecycle)). [CancellationToken](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html)
- **The worker's job tokens.** Each job's token is a child of the worker's `abandon` token, so `cancel_job_ids`, a `409 lease_lost` and the end of the shutdown drain all stop a job the same way: `run_job` `select!`s on the token and the model's future, and dropping that future is the cancellation; nothing is sent for a cancelled job ([backend worker 7](../../../docs/backend/worker.md#7-cancellation)).

```rust
use std::time::Duration;

use tokio_util::sync::CancellationToken;

#[test]
fn cancelling_the_parent_cancels_every_child() {
    let root = CancellationToken::new();
    let child = root.child_token();
    let grandchild = child.child_token();
    root.cancel();
    assert!(child.is_cancelled());
    assert!(grandchild.is_cancelled());
}

#[test]
fn cancelling_a_child_leaves_the_parent_and_siblings_alone() {
    let root = CancellationToken::new();
    let a = root.child_token();
    let b = root.child_token();
    a.cancel();
    assert!(a.is_cancelled());
    assert!(!root.is_cancelled());
    assert!(!b.is_cancelled());
}

#[test]
fn clones_share_one_state() {
    let token = CancellationToken::new();
    let clone = token.clone();
    clone.cancel();
    assert!(token.is_cancelled());
}

#[test]
fn dropping_a_token_cancels_nothing() {
    let root = CancellationToken::new();
    let child = root.child_token();
    drop(child.clone()); // a clone of the child
    drop(root.child_token()); // another child
    assert!(!root.is_cancelled());
    assert!(!child.is_cancelled());

    // Dropping every handle to a parent doesn't cancel its child either,
    // and the child still hears a cancel from further up.
    let top = CancellationToken::new();
    let middle = top.child_token();
    let leaf = middle.child_token();
    drop(middle);
    assert!(!leaf.is_cancelled());
    top.cancel();
    assert!(leaf.is_cancelled());
}

#[test]
fn a_drop_guard_cancels_when_dropped_unless_disarmed() {
    let token = CancellationToken::new();
    let guard = token.clone().drop_guard();
    drop(guard);
    assert!(token.is_cancelled());

    let token = CancellationToken::new();
    let guard = token.clone().drop_guard();
    let _token_back: CancellationToken = guard.disarm();
    assert!(!token.is_cancelled());
}

#[test]
fn a_child_of_a_cancelled_token_starts_cancelled() {
    let root = CancellationToken::new();
    root.cancel();
    assert!(root.child_token().is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn cancelled_owned_moves_into_a_static_task() {
    let token = CancellationToken::new();
    // `cancelled()` borrows the token; `cancelled_owned()` owns a clone, so the
    // future is 'static and can go to `tokio::spawn`.
    let waiter = tokio::spawn(token.clone().cancelled_owned());
    token.cancel();
    waiter.await.expect("the waiter task does not panic");
}

#[tokio::test(start_paused = true)]
async fn run_until_cancelled_drops_the_future() {
    let token = CancellationToken::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        canceller.cancel();
    });
    let out = token
        .run_until_cancelled(tokio::time::sleep(Duration::from_secs(60)))
        .await;
    assert_eq!(out, None);

    // On a token that is already cancelled it returns None without polling the future.
    let out = token.run_until_cancelled(async { 7 }).await;
    assert_eq!(out, None);

    // The future wins when it completes first.
    let fresh = CancellationToken::new();
    assert_eq!(fresh.run_until_cancelled(async { 7 }).await, Some(7));
}
```
