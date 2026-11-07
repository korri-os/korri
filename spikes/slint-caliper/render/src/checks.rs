//! Caliper src/render/checks.js v1 report, baseline identity and approval semantics.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn check(name: &str, status: &str, detail: impl Into<String>) -> Value {
    json!({"name":name,"status":status,"detail":detail.into()})
}
fn key(result: &Value) -> Result<String> {
    Ok(digest(&serde_json::to_vec(&json!([
        "korri",
        result["part"],
        result["state"],
        result["device"]
    ]))?))
}
fn image_digest(result: &Value, field: &str) -> Result<String> {
    let path = result[field].as_str().context("Image path missing")?;
    if path.is_empty() {
        return Ok(digest(b""));
    }
    Ok(digest(&std::fs::read(path)?))
}
fn baseline(result: &Value, dir: Option<&Path>, stable: bool) -> Value {
    if !stable {
        return check(
            "baseline",
            "Inconclusive",
            "Broken or unstable renders cannot establish an image baseline.",
        );
    }
    let Some(dir) = dir else {
        return check("baseline", "NotRun", "No baseline directory supplied.");
    };
    let read = || -> Result<Value> {
        let path = dir.join(format!("{}.json", key(result)?));
        if !path.exists() {
            return Ok(check(
                "baseline",
                "Review",
                "No accepted baseline. Inspect both images before approving this report.",
            ));
        }
        let baseline: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        if baseline["version"] != 1
            || baseline["project"] != "korri"
            || ["part", "state", "device"]
                .iter()
                .any(|k| baseline[k] != result[k])
        {
            bail!("Invalid baseline identity");
        }
        let sha = baseline["sha256"]
            .as_str()
            .context("Baseline digest missing")?;
        if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("Invalid baseline digest");
        }
        let image = dir.join(format!("{sha}.png"));
        if digest(&std::fs::read(&image)?) != sha {
            bail!("Baseline image changed");
        }
        let mut value = if baseline["environment"] != result["environment"] {
            check(
                "baseline",
                "Inconclusive",
                "Baseline renderer/platform/check version differs. Review a new baseline.",
            )
        } else {
            let old = image::open(&image)?.to_rgb8();
            let new = image::open(result["png"].as_str().context("PNG missing")?)?.to_rgb8();
            let same_size = old.dimensions() == new.dimensions();
            let changed = if same_size {
                old.pixels()
                    .zip(new.pixels())
                    .filter(|(a, b)| a != b)
                    .count()
            } else {
                new.width() as usize * new.height() as usize
            };
            let equal = same_size
                && changed == 0
                && baseline["frame"] == result["frame"]
                && baseline["viewport"] == result["viewport"];
            check(
                "baseline",
                if equal { "Passed" } else { "Review" },
                format!(
                    "{changed} differing pixels; dimensions match: {same_size}. {}",
                    if equal {
                        "Matches accepted image."
                    } else {
                        "Review accepted and current images."
                    }
                ),
            )
        };
        value["image"] = json!(image);
        Ok(value)
    };
    read().unwrap_or_else(|e| {
        check(
            "baseline",
            "Inconclusive",
            format!("Cannot read baseline: {e:#}"),
        )
    })
}
pub fn compare(first: &mut Value, second: &Value, baselines: Option<&Path>) -> Result<()> {
    let sha = image_digest(first, "png")?;
    let repeat_sha = image_digest(second, "png")?;
    let broken = first["frame"] == "Failed" || second["frame"] == "Failed";
    let empty = first["frame"] == "Empty" || second["frame"] == "Empty";
    let stable = !broken && sha == repeat_sha && first["frame"] == second["frame"];
    first["sha256"] = json!(sha);
    first["repeatSha256"] = json!(repeat_sha);
    first["repeatPng"] = second["png"].clone();
    let spilled = !first["spill"].is_null() || !second["spill"].is_null();
    let mut checks = vec![
        check("render", if broken {"Failed"} else if empty {"Review"} else {"Passed"}, if broken { format!("First: {}; repeat: {}",first["problems"],second["problems"]) } else if empty {"Uniform RGB565 buffer: surface-probe's empty-draw rule. Review whether intentionally empty.".into()} else {"Both samples rendered nonuniform content (surface-probe rule).".into()}),
        check("browser","NotRun","Native Slint, not a browser; browser console/network audits have no equivalent here. Compiler/render failures are in render."),
        check("spill",if broken {"Inconclusive"} else if spilled {"Review"} else {"Passed"}, if broken {"Broken render; geometry unavailable.".into()} else if spilled {format!("ElementHandle boxes extend outside the window (0.5 px tolerance). Scrolling can be intentional. First: {}; repeat: {}", first["spill"],second["spill"])} else {"No measured element boxes outside the window.".into()}),
    ];
    for (field, name) in [
        ("accessibilityCheck", "accessibility"),
        ("focusCheck", "focus"),
    ] {
        let a = &first[field];
        let b = &second[field];
        checks.push(if a.is_null() || b.is_null() {
            check(
                name,
                "Inconclusive",
                "Broken render; native tree check unavailable.",
            )
        } else if a["status"] == "Failed" || b["status"] == "Failed" {
            check(
                name,
                "Failed",
                format!("First: {}; repeat: {}", a["detail"], b["detail"]),
            )
        } else {
            a.clone()
        });
    }
    checks.push(check("determinism",if stable {"Passed"} else {"Inconclusive"},if stable {"Two fresh component instances match byte-for-byte at a frozen animation tick. This is a sample, not proof of determinism."} else {"Broken or differing renders. Review both images; timers/animations may differ."}));
    checks.push(baseline(first, baselines, stable));
    first["checks"] = json!(checks);
    Ok(())
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}
pub fn approve(report_path: &Path, directory: &Path) -> Result<Value> {
    let report: Value = serde_json::from_slice(&std::fs::read(report_path)?)?;
    if report["version"] != 1
        || report["project"] != "korri"
        || report["environment"] != super::ENVIRONMENT
    {
        bail!("Expected this spike's native Caliper v1 report");
    }
    let results = report["results"]
        .as_array()
        .filter(|a| !a.is_empty())
        .context("Report needs nonempty results")?;
    let mut seen = std::collections::BTreeSet::new();
    let mut entries = Vec::new();
    // Validate every saved image before changing any baseline; never render here.
    for result in results {
        for field in ["part", "state", "device", "png", "repeatPng"] {
            if result[field].as_str().is_none_or(str::is_empty) {
                bail!("Missing {field}");
            }
        }
        if result.get("take").is_some() {
            bail!("Take images cannot become product baselines");
        }
        let key = key(result)?;
        if !seen.insert(key.clone()) {
            bail!("Duplicate state/device in report");
        }
        let checks = result["checks"].as_array().context("Checks missing")?;
        let status = |name| -> Option<&str> {
            let found: Vec<_> = checks.iter().filter(|c| c["name"] == name).collect();
            (found.len() == 1)
                .then(|| found[0]["status"].as_str())
                .flatten()
        };
        // Browser is deliberately NotRun for native Slint. No other finding is suppressed.
        if !matches!(result["frame"].as_str(), Some("Rendered" | "Empty"))
            || status("determinism") != Some("Passed")
            || status("browser") != Some("NotRun")
            || !matches!(status("render"), Some("Passed" | "Review"))
        {
            bail!("Cannot approve broken or unstable render");
        }
        let bytes = std::fs::read(result["png"].as_str().unwrap())?;
        let sha = digest(&bytes);
        if result["sha256"] != sha
            || result["repeatSha256"] != sha
            || image_digest(result, "repeatPng")? != sha
        {
            bail!("Report images changed or differ");
        }
        let image = image::load_from_memory(&bytes)?;
        if result["viewport"]["width"].as_u64() != Some(image.width() as u64)
            || result["viewport"]["height"].as_u64() != Some(image.height() as u64)
        {
            bail!("Image viewport differs from report");
        }
        let record = json!({"version":1,"project":report["project"],"environment":report["environment"],"part":result["part"],"state":result["state"],"device":result["device"],"frame":result["frame"],"viewport":result["viewport"],"sha256":sha});
        entries.push((key, record, bytes, sha));
    }
    std::fs::create_dir_all(directory)?;
    for (key, record, bytes, sha) in &entries {
        atomic(&directory.join(format!("{sha}.png")), bytes)?;
        atomic(
            &directory.join(format!("{key}.json")),
            &serde_json::to_vec_pretty(record)?,
        )?;
    }
    Ok(
        json!({"approved":entries.len(),"baselines":std::env::current_dir()?.join(directory),"scope":"Visual baselines only. Other findings remain; native browser audit is NotRun."}),
    )
}
