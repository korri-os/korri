// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Band threads that stay: started once and parked between frames, instead of one new
//! thread per band thread and frame (`parallel`).
//!
//! [`scope`] works like `std::thread::scope`: a job may borrow from the caller, and `scope`
//! returns, or unwinds, only once every job it spawned has ended. While it waits, the
//! calling thread runs its own jobs that no pool thread took yet, so a scope opened while
//! every pool thread is busy (a layer image drawn in bands while the frame's band threads
//! wait for the frame's draw calls) still ends.

#![allow(unsafe_code)] // One lifetime erasure, justified where it is written.

use std::boxed::Box;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

type Job = Box<dyn FnOnce() + Send + 'static>;

/// At most this many pool threads.
const MAX_THREADS: usize = 8;

struct Pool {
    state: Mutex<State>,
    ready: Condvar,
}

#[derive(Default)]
struct State {
    jobs: VecDeque<(Arc<Pending>, Job)>,
    threads: usize,
    idle: usize,
}

/// The jobs of one scope not ended yet.
#[derive(Default)]
struct Pending {
    count: Mutex<usize>,
    ended: Condvar,
    panicked: AtomicBool,
}

impl Pending {
    fn end_one(&self) {
        let mut count = self.count.lock().unwrap();
        *count -= 1;
        if *count == 0 {
            self.ended.notify_all();
        }
    }
}

fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| Pool { state: Mutex::new(State::default()), ready: Condvar::new() })
}

fn run(pending: &Pending, job: Job) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_err() {
        pending.panicked.store(true, Ordering::Relaxed);
    }
    pending.end_one();
}

fn worker() {
    let pool = pool();
    let mut state = pool.state.lock().unwrap();
    loop {
        if let Some((pending, job)) = state.jobs.pop_front() {
            drop(state);
            run(&pending, job);
            state = pool.state.lock().unwrap();
            continue;
        }
        state.idle += 1;
        state = pool.ready.wait(state).unwrap();
        state.idle -= 1;
    }
}

/// Where jobs borrowing from `'env` are spawned (`scope`).
pub(super) struct Scope<'env> {
    pending: Arc<Pending>,
    _env: core::marker::PhantomData<&'env mut &'env ()>,
}

impl<'env> Scope<'env> {
    /// Run `job` on a pool thread, or on the scope's thread if none takes it first.
    pub(super) fn spawn(&self, job: impl FnOnce() + Send + 'env) {
        *self.pending.count.lock().unwrap() += 1;
        let job: Box<dyn FnOnce() + Send + 'env> = Box::new(job);
        // SAFETY: `scope` does not return or unwind before every job it spawned ended
        // (`Wait`), so nothing the job borrows for `'env` is used after it is gone.
        let job: Job = unsafe { core::mem::transmute(job) };
        let pool = pool();
        let mut state = pool.state.lock().unwrap();
        state.jobs.push_back((self.pending.clone(), job));
        if state.jobs.len() > state.idle && state.threads < MAX_THREADS {
            state.threads += 1;
            let started = std::thread::Builder::new().name("slint-band".into()).spawn(worker);
            if started.is_err() {
                // The scope's thread runs the job while it waits.
                state.threads -= 1;
            }
        }
        drop(state);
        pool.ready.notify_one();
    }
}

/// Wait for every job of the scope; run those no pool thread took.
struct Wait<'a>(&'a Arc<Pending>);

impl Drop for Wait<'_> {
    fn drop(&mut self) {
        let pending = self.0;
        loop {
            let mine = {
                let pool = pool();
                let mut state = pool.state.lock().unwrap();
                let position = state.jobs.iter().position(|(p, _)| Arc::ptr_eq(p, pending));
                position.and_then(|i| state.jobs.remove(i))
            };
            match mine {
                Some((pending, job)) => run(&pending, job),
                None => break,
            }
        }
        let mut count = pending.count.lock().unwrap();
        while *count > 0 {
            count = pending.ended.wait(count).unwrap();
        }
    }
}

/// Run `f` with a scope whose jobs may borrow what `f` may; return once they all ended.
pub(super) fn scope<'env, R>(f: impl FnOnce(&Scope<'env>) -> R) -> R {
    let scope = Scope { pending: Arc::new(Pending::default()), _env: core::marker::PhantomData };
    let result = {
        let _wait = Wait(&scope.pending);
        f(&scope)
    };
    if scope.pending.panicked.load(Ordering::Relaxed) {
        panic!("a band thread panicked");
    }
    result
}
