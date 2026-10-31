//! THROWAWAY: each package has a separate QuickJS heap. State never leaves it.
use anyhow::{anyhow, Context as _, Result};
use rquickjs::{Context, Runtime};
use std::path::Path;

pub struct Logic {
    context: Context,
    // Drop the context before its runtime.
    _runtime: Runtime,
}

impl Logic {
    pub fn load(package: &Path, model: &str) -> Result<Self> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let source = std::fs::read(package.join("logic.js")).context("read logic.js")?;
        context.with(|ctx| -> Result<()> {
            let run = || -> rquickjs::Result<()> {
                ctx.eval::<(), _>(source)?;
                let model = ctx.json_parse(model)?;
                ctx.globals().set("__shellModel", model)?;
                ctx.eval::<(), _>(
                    r#"
                    globalThis.__shellDispatch = (() => {
                        const model = globalThis.__shellModel;
                        delete globalThis.__shellModel;
                        let state = surface.init(model);
                        return message => {
                            if (message !== null) state = surface.update(state, message, model);
                            return JSON.stringify(surface.view(state, model));
                        };
                    })();
                "#,
                )
            };
            run().map_err(|e| anyhow!("QuickJS load: {e}; exception: {:?}", ctx.catch()))
        })?;
        Ok(Self {
            context,
            _runtime: runtime,
        })
    }

    pub fn view(&self, action: Option<&str>) -> Result<serde_json::Value> {
        self.context.with(|ctx| {
            let message = match action {
                Some(action) => serde_json::json!({"_tag":"Pressed", "action":action}),
                None => serde_json::Value::Null,
            };
            let call = format!("__shellDispatch({message})");
            let text: String = ctx
                .eval(call)
                .map_err(|e| anyhow!("QuickJS view/update: {e}; exception: {:?}", ctx.catch()))?;
            serde_json::from_str(&text).context("view must return JSON")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_retains_non_json_values_and_contexts_are_fresh() -> Result<()> {
        let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("surfaces/_test/a");
        let model = r#"{"catalog":{"games":[{"coverArtUrl":"unused"}]}}"#;
        let first = Logic::load(&package, model)?;
        assert_eq!(first.view(None)?["position"], 0);
        assert_eq!(first.view(Some("right"))?["position"], 1);
        assert_eq!(first.view(Some("right"))?["position"], 2);
        assert_eq!(Logic::load(&package, model)?.view(None)?["position"], 0);
        Ok(())
    }
}
