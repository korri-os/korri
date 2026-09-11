//! One real scheduler for policy and conformance evaluations. No JS reference
//! escapes the context lifetime; close clears callback/argument roots on all exits.
use rquickjs::{Ctx, Function, Object};
use serde_json::Value as Json;
use std::time::{Duration, Instant};

pub struct Timers<'js> {
    controller: Object<'js>,
    deadline: Instant,
}
impl<'js> Timers<'js> {
    pub fn install(ctx: &Ctx<'js>, started: Instant, deadline: Instant) -> rquickjs::Result<Self> {
        let install: Function = ctx.eval(include_str!("timers.js"))?;
        let now = Function::new(ctx.clone(), move || started.elapsed().as_secs_f64() * 1000.)?;
        Ok(Self {
            controller: install.call((now,))?,
            deadline,
        })
    }
    pub fn stats(&self) -> rquickjs::Result<Json> {
        let stats: Function = self.controller.get("stats")?;
        let values: Object = stats.call(())?;
        let mut output = serde_json::Map::new();
        for key in [
            "scheduled",
            "fired",
            "cancelled",
            "pending",
            "highWater",
            "limit",
        ] {
            output.insert(key.into(), values.get::<_, u64>(key)?.into());
        }
        Ok(output.into())
    }
    pub fn drain(&self, ctx: &Ctx<'js>) -> Result<(), String> {
        let next: Function = self
            .controller
            .get("next")
            .map_err(|e| super::failure(ctx, e))?;
        let tick: Function = self
            .controller
            .get("tick")
            .map_err(|e| super::failure(ctx, e))?;
        loop {
            if Instant::now() >= self.deadline {
                return Err("Probe total evaluation deadline exceeded".into());
            }
            // Drain actual QuickJS promise jobs between macrotasks, under the
            // same interrupt deadline. No async-I/O or separate test scheduler.
            let job = unsafe {
                let mut job_context = std::ptr::null_mut();
                rquickjs::qjs::JS_ExecutePendingJob(
                    rquickjs::qjs::JS_GetRuntime(ctx.as_raw().as_ptr()),
                    &mut job_context,
                )
            };
            if job < 0 {
                return Err(super::failure(ctx, rquickjs::Error::Exception));
            }
            if job > 0 {
                continue;
            }
            let wait: f64 = next.call(()).map_err(|e| super::failure(ctx, e))?;
            if wait < 0. {
                return Ok(());
            }
            if wait > 0. {
                std::thread::sleep(
                    Duration::from_secs_f64(wait / 1000.)
                        .min(self.deadline.saturating_duration_since(Instant::now())),
                );
            } else {
                tick.call::<_, ()>(()).map_err(|e| super::failure(ctx, e))?;
            }
        }
    }
    pub fn close(&self) -> rquickjs::Result<usize> {
        let close: Function = self.controller.get("close")?;
        close.call(())
    }
}
