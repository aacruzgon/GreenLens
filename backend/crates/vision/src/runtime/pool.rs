//! A fixed pool of sessions for `api`'s CPU models (the face detector, the text embedder).
//!
//! `ort` rc.14's `Session::run` takes `&mut self`, so a session serves one caller at a time.
//! `Session` is `Send + Sync`, so N sessions in a `Mutex<Vec<_>>` let N blocking threads run the
//! same model at once; the (N+1)th waits on a `Condvar`. Callers are already on blocking threads
//! (`spawn_blocking`), where waiting on a `std` lock is allowed.
//!
//! The perception worker doesn't use this pool: its sessions live on one inference thread
//! (`perception::inference`).

use std::{
    ops::{Deref, DerefMut},
    sync::{Condvar, Mutex, PoisonError},
};

use ort::session::Session;

/// N interchangeable values handed out one caller at a time. Generic so the checkout logic is
/// tested without ONNX Runtime; production uses [`SessionPool`].
#[derive(Debug)]
pub struct Pool<T> {
    name: &'static str,
    idle: Mutex<Vec<T>>,
    returned: Condvar,
    size: usize,
}

/// A pool of ONNX Runtime sessions of one model.
pub type SessionPool = Pool<Session>;

/// A checked-out value. It borrows the pool for `'p`, so it can't outlive the pool, and gives the
/// value back in `Drop`, however the caller's scope ends (an early return, `?`, a panic).
///
/// `'p` is the borrow of the pool: a caller in `spawn_blocking` owns an `Arc` of whatever holds
/// the pool (moved into the closure, so the closure is `'static`) and checks out inside the
/// closure; the guard then lives no longer than that closure's borrow of its own `Arc`.
#[derive(Debug)]
pub struct Pooled<'p, T> {
    pool: &'p Pool<T>,
    value: Option<T>,
}

impl<T> Pool<T> {
    /// A pool of the given values. `name` appears in logs.
    pub fn new(name: &'static str, values: Vec<T>) -> Self {
        let size = values.len();
        Self {
            name,
            idle: Mutex::new(values),
            returned: Condvar::new(),
            size,
        }
    }

    /// How many values the pool holds in all.
    pub const fn size(&self) -> usize {
        self.size
    }

    /// The pool's name.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Waits for a free value. Blocks the thread: call only from a blocking thread.
    pub fn checkout(&self) -> Pooled<'_, T> {
        // The lock guards a Vec push or pop, so a panic elsewhere can't leave it inconsistent.
        let mut idle = self.idle.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(value) = idle.pop() {
                return Pooled {
                    pool: self,
                    value: Some(value),
                };
            }
            idle = self
                .returned
                .wait(idle)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn give_back(&self, value: T) {
        self.idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(value);
        self.returned.notify_one();
    }
}

impl<T> Deref for Pooled<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // reason: `value` is Some from checkout until drop.
        self.value.as_ref().expect("checked-out value")
    }
}

impl<T> DerefMut for Pooled<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // reason: `value` is Some from checkout until drop.
        self.value.as_mut().expect("checked-out value")
    }
}

impl<T> Drop for Pooled<'_, T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            self.pool.give_back(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    #[test]
    fn never_hands_out_more_than_it_holds() {
        let pool = Arc::new(Pool::new("test", vec![0u32, 0]));
        let (now, peak) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let (pool, now, peak) = (pool.clone(), now.clone(), peak.clone());
                std::thread::spawn(move || {
                    let mut value = pool.checkout();
                    let running = now.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(running, Ordering::SeqCst);
                    *value += 1;
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    now.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(peak.load(Ordering::SeqCst), 2);
        let total: u32 = pool.idle.lock().unwrap().iter().sum();
        assert_eq!(total, 8);
    }

    #[test]
    fn a_panicking_caller_still_returns_its_value() {
        let pool = Arc::new(Pool::new("test", vec![1u8]));
        let p = pool.clone();
        let _ = std::thread::spawn(move || {
            let _held = p.checkout();
            panic!("model failed");
        })
        .join();
        assert_eq!(*pool.checkout(), 1);
    }
}
