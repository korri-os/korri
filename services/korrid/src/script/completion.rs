//! One completion policy for every interpreter consumer. Timer references live
//! only in the fresh context; rejection identities are released before runtime
//! disposal. No user code runs as cleanup after interruption.

use rquickjs::{Array, Ctx, Function, Object, Persistent, Promise, Runtime, Value};
use std::{
    cell::RefCell,
    collections::HashSet,
    rc::Rc,
    time::{Duration, Instant},
};

pub(super) struct Rejections {
    pending: Rc<RefCell<HashSet<Persistent<Value<'static>>>>>,
}

impl Rejections {
    pub(super) fn install(runtime: &Runtime) -> Self {
        let pending = Rc::new(RefCell::new(HashSet::new()));
        let tracked = Rc::clone(&pending);
        runtime.set_host_promise_rejection_tracker(Some(Box::new(
            move |ctx, promise, _reason, handled| {
                // Retain identity, not a raw pointer that GC can free and reuse.
                // Never inspect/coerce the rejection payload in this callback.
                let identity = Persistent::save(&ctx, promise);
                if handled {
                    tracked.borrow_mut().remove(&identity);
                } else {
                    tracked.borrow_mut().insert(identity);
                }
            },
        )));
        Self { pending }
    }

    fn checkpoint(&self) -> Result<(), String> {
        if self.pending.borrow().is_empty() {
            Ok(())
        } else {
            Err("plugin has an unhandled promise rejection".into())
        }
    }
}

impl Drop for Rejections {
    fn drop(&mut self) {
        // This guard is constructed after Runtime and before Context. Clear all
        // Persistent values before runtime destruction, including error exits.
        self.pending.borrow_mut().clear();
    }
}

pub(super) struct Completion<'js> {
    queue: Array<'js>,
    next: Function<'js>,
    tick: Function<'js>,
    deadline: Instant,
}

impl Drop for Completion<'_> {
    fn drop(&mut self) {
        // This is a private native array with its own nonconfigurable length
        // and null prototype. Neither plugins nor proxies can intercept this
        // write. It releases callback roots without running JS after timeout.
        let _ = self.queue.as_object().set("length", 0u32);
    }
}

impl<'js> Completion<'js> {
    pub(super) fn install(
        ctx: &Ctx<'js>,
        started: Instant,
        deadline: Instant,
    ) -> Result<Self, String> {
        let install: Function = ctx
            .eval(include_str!("completion.js"))
            .map_err(|_| "plugin timer initialization failed")?;
        let elapsed = Function::new(ctx.clone(), move || started.elapsed().as_secs_f64() * 1000.)
            .map_err(|_| "plugin timer clock unavailable")?;
        let queue = Array::new(ctx.clone()).map_err(|_| "plugin timer queue unavailable")?;
        let controller: Object = install
            .call((elapsed, queue.clone()))
            .map_err(|_| "plugin timer initialization failed")?;
        Ok(Self {
            queue,
            next: controller
                .get("next")
                .map_err(|_| "plugin timer initialization failed")?,
            tick: controller
                .get("tick")
                .map_err(|_| "plugin timer initialization failed")?,
            deadline,
        })
    }

    pub(super) fn initialize(&self, ctx: &Ctx<'js>, promise: &Promise<'js>) -> Result<(), String> {
        loop {
            self.check_deadline()?;
            if let Some(result) = promise.result::<()>() {
                return result.map_err(|_| "plugin module initialization failed".into());
            }
            if !self.run_job(ctx)? {
                return Err("plugin module initialization is pending without promise jobs; timer-dependent initialization is unsupported".into());
            }
        }
    }

    pub(super) fn drain(&self, ctx: &Ctx<'js>, rejections: &Rejections) -> Result<(), String> {
        loop {
            self.check_deadline()?;
            if self.run_job(ctx)? {
                continue;
            }
            // A handler queued in the same promise-job checkpoint can remove
            // the identity. A handler in a later timer is too late.
            rejections.checkpoint()?;
            let wait: f64 = self
                .next
                .call(())
                .map_err(|_| "plugin timer inspection failed")?;
            if wait < 0. {
                return self.check_deadline();
            }
            if wait > 0. {
                std::thread::sleep(
                    Duration::from_secs_f64(wait / 1000.)
                        .min(self.deadline.saturating_duration_since(Instant::now())),
                );
            } else {
                self.tick
                    .call::<_, ()>(())
                    .map_err(|_| "plugin timer callback failed")?;
            }
        }
    }

    pub(super) fn check_deadline(&self) -> Result<(), String> {
        if Instant::now() >= self.deadline {
            Err("plugin execution deadline exceeded".into())
        } else {
            Ok(())
        }
    }

    fn run_job(&self, ctx: &Ctx<'js>) -> Result<bool, String> {
        // Ctx::execute_pending_job returns only a bool, merging failure with an
        // empty queue. Keep QuickJS's negative error result distinct here. The
        // context owns the runtime lock throughout this call.
        let result = unsafe {
            let mut job_context = std::ptr::null_mut();
            rquickjs::qjs::JS_ExecutePendingJob(
                rquickjs::qjs::JS_GetRuntime(ctx.as_raw().as_ptr()),
                &mut job_context,
            )
        };
        match result {
            n if n < 0 => Err("plugin promise job failed".into()),
            0 => Ok(false),
            _ => Ok(true),
        }
    }
}
