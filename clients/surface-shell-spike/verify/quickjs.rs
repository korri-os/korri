//! Spike-only bounded runner. No host functions or I/O are exposed.
use crate::allocation::{Measured, Meter};
use anyhow::{anyhow, bail, Result};
use rquickjs::{Context, Runtime};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

pub struct Runner {
    context: Context,
    _runtime: Runtime,
    meter: Rc<Meter>,
    deadline: Rc<Cell<Instant>>,
    budget: Duration,
    pub max_call_ms: f64,
}
impl Runner {
    pub fn new(memory: usize, budget: Duration) -> Result<Self> {
        let meter = Rc::new(Meter {
            current: Cell::new(0),
            peak: Cell::new(0),
            limit: memory,
        });
        let runtime = Runtime::new_with_alloc(Measured(meter.clone()))?;
        runtime.set_memory_limit(memory);
        let deadline = Rc::new(Cell::new(Instant::now() + budget));
        let clock = deadline.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= clock.get())));
        let context = Context::full(&runtime)?;
        Ok(Self {
            context,
            _runtime: runtime,
            meter,
            deadline,
            budget,
            max_call_ms: 0.,
        })
    }
    pub fn call(&mut self, source: &str) -> Result<(String, f64)> {
        let start = Instant::now();
        self.deadline.set(start + self.budget);
        let result = self.context.with(|ctx| {
            ctx.eval::<String, _>(source)
                .map_err(|e| anyhow!("QuickJS: {e}; exception: {:?}", ctx.catch()))
        });
        let ms = start.elapsed().as_secs_f64() * 1000.;
        self.max_call_ms = self.max_call_ms.max(ms);
        if start.elapsed() > self.budget {
            bail!("call time budget exceeded ({ms:.3} ms)");
        }
        Ok((result?, ms))
    }
    pub fn peak_bytes(&self) -> usize {
        self.meter.peak.get()
    }
    pub fn load(&mut self, source: &str, model: &str) -> Result<()> {
        // The clock and random generator cannot change a replay. Explicit Date
        // construction remains available because Pico formats model timestamps.
        self.call("Date.now = () => { throw Error('clock forbidden'); }; Math.random = () => { throw Error('random forbidden'); }; ''")?;
        self.call(&format!("{source}\n;''"))?;
        self.call(&format!(
            "globalThis.__model = JSON.parse({}); ''",
            serde_json::to_string(model)?
        ))?;
        Ok(())
    }
    pub fn init(&mut self) -> Result<f64> {
        Ok(self
            .call("globalThis.__state = surface.init(__model); ''")?
            .1)
    }
    pub fn update(&mut self, action: &str) -> Result<f64> {
        Ok(self
            .call(&format!(
                "__state = surface.update(__state, {{_tag:'Pressed', action:{}}}, __model); ''",
                serde_json::to_string(action)?
            ))?
            .1)
    }
    pub fn view(&mut self) -> Result<(serde_json::Value, usize, f64)> {
        let (text, ms) = self.call("JSON.stringify(surface.view(__state, __model))")?;
        let size = text.len();
        Ok((serde_json::from_str(&text)?, size, ms))
    }
}
