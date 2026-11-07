//! Save a real Zone/Pico view using the shell engine, not a new fixture schema.
//! cargo run --example view -- PACKAGE SCENARIO.json OUTPUT.json
use anyhow::{Context, Result};
use korri_shell::engine::{Engine, Limits};
use serde_json::{json, Value};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(args.len() == 3, "PACKAGE SCENARIO.json OUTPUT.json");
    let source =
        korri_shell::engine::read_source(&std::path::Path::new(&args[0]).join("logic.js"))?;
    let scenario: Value = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let mut model = scenario["models"]
        .get(0)
        .context("scenario models[0]")?
        .clone();
    // surface-probe treats scenario games as the catalog's input, not its wire window.
    if let Some(games) = model["catalog"]["games"].as_array() {
        model["catalog"] = korri_shell::catalog::window(
            games,
            &korri_shell::catalog::default_query(),
            &json!({"_tag":"Start"}),
            korri_shell::catalog::DEFAULT_COUNT,
        );
    }
    let mut engine = Engine::load(&source, Limits::default())?;
    engine.set_model(&serde_json::to_string(&model)?)?;
    engine.init(&korri_shell::engine::effective_preferences(
        engine.declaration(),
        &Default::default(),
    ))?;
    engine.update(&json!({"_tag":"Resized","width":640,"height":480}))?;
    std::fs::write(&args[2], serde_json::to_vec_pretty(&engine.view()?)?)?;
    Ok(())
}
