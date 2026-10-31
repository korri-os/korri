use crate::{index, quickjs::Runner};
use anyhow::{Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, time::Duration};

pub fn one(package: &Path, model: &Path, n: usize) -> Result<()> {
    let mut runner = Runner::new(128 * 1024 * 1024, Duration::from_secs(30))?;
    let mut metrics = json!({});
    let outcome = (|| -> Result<()> {
        runner.load(
            &std::fs::read_to_string(package.join("logic.js"))?,
            &std::fs::read_to_string(model)?,
        )?;
        metrics["init_ms"] = json!(runner.init()?);
        metrics["update_right_ms"] = json!(runner.update("right")?);
        let (_, bytes, view) = runner.view()?;
        metrics["view_ms"] = json!(view);
        metrics["view_json_bytes"] = json!(bytes);
        Ok(())
    })();
    let mut result = json!({"package":package.file_name().context("package name")?.to_string_lossy(),"games":n,"pass":outcome.is_ok(),"peak_quickjs_backing_bytes":runner.peak_bytes(),"rss_kib":index::rss("VmRSS:"),"hwm_kib":index::rss("VmHWM:")});
    match outcome {
        Ok(()) => {}
        Err(e) => result["error"] = json!(format!("{e:#}")),
    }
    result["metrics"] = metrics;
    result["max_call_ms"] = json!(runner.max_call_ms);
    println!("{result}");
    Ok(())
}

pub fn run(root: &Path, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out.join("art"))?;
    let mut model: Value =
        serde_json::from_slice(&std::fs::read(root.join("fixtures/model.json"))?)?;
    let mut originals = model["catalog"]["games"]
        .as_array()
        .context("fixture games")?
        .clone();
    let mut art = BTreeMap::new();
    for game in &mut originals {
        if let Some(url) = game["coverArtUrl"].as_str() {
            let next = art.len();
            let path = out.canonicalize()?.join(format!("art/cover-{next}.png"));
            let path = art.entry(url.to_owned()).or_insert(path);
            let (_, data) = url.split_once(',').context("fixture data URL")?;
            std::fs::write(
                &*path,
                base64::engine::general_purpose::STANDARD.decode(data)?,
            )?;
            game["coverArtUrl"] = json!(path.to_str().context("art path")?);
        }
    }
    for n in [1000, 10000, 50000] {
        model["catalog"]["games"] = Value::Array(
            (0..n)
                .map(|i| {
                    let mut game = originals[i % originals.len()].clone();
                    game["id"] = json!(format!("{}-{i}", game["id"].as_str().unwrap_or("game")));
                    game["title"] =
                        json!(format!("{} {i}", game["title"].as_str().unwrap_or("Game")));
                    game
                })
                .collect(),
        );
        let model_path = out.join(format!("model-{n}.json"));
        std::fs::write(&model_path, serde_json::to_vec(&model)?)?;
        for name in ["zone", "pico"] {
            let status = crate::self_command()?
                .arg("large-one")
                .arg(root.join("surfaces").join(name))
                .arg(&model_path)
                .arg(n.to_string())
                .status()?;
            anyhow::ensure!(status.success(), "large-one child failed {status}");
        }
    }
    Ok(())
}
