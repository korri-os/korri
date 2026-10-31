//! Spike-only installed-package workload. Summary is excluded from its own digest.
use crate::conformance::{self, SUMMARY};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    hint::black_box,
    path::{Path, PathBuf},
    time::Instant,
};

pub fn files(root: &Path) -> Result<Vec<PathBuf>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        let mut entries = std::fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            if path == root.join(SUMMARY) {
                continue;
            }
            let ty = entry.file_type()?;
            if ty.is_symlink() {
                bail!("symlinks not supported by spike digest: {}", path.display());
            }
            if ty.is_dir() {
                walk(root, &path, out)?;
            } else if ty.is_file() {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = vec![];
    walk(root, root, &mut out)?;
    Ok(out)
}

pub fn digest(root: &Path, blake: bool) -> Result<String> {
    let mut sha = Sha256::new();
    let mut b3 = blake3::Hasher::new();
    for file in files(root)? {
        let name = file.strip_prefix(root)?.to_string_lossy();
        let data = std::fs::read(&file)?;
        for bytes in [
            &(name.len() as u64).to_le_bytes()[..],
            name.as_bytes(),
            &(data.len() as u64).to_le_bytes()[..],
            &data,
        ] {
            if blake {
                b3.update(bytes);
            } else {
                sha.update(bytes);
            }
        }
    }
    Ok(if blake {
        b3.finalize().to_hex().to_string()
    } else {
        format!("{:x}", sha.finalize())
    })
}

pub fn rss(field: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with(field))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse().ok())
        })
        .unwrap_or(0)
}

pub fn phase(method: &str, root: &Path, model: &Path) -> Result<Value> {
    let mut dirs: Vec<_> = std::fs::read_dir(root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    dirs.sort();
    let start = Instant::now();
    let mut peak_worker = 0;
    let mut failures = 0;
    let jobs = std::env::var("SURFACE_VERIFY_JOBS")
        .ok()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(1);
    anyhow::ensure!(jobs > 0, "SURFACE_VERIFY_JOBS must be positive");
    if method == "verify" {
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..jobs)
                .map(|job| {
                    let dirs = &dirs;
                    scope.spawn(move || -> Result<(u64, usize)> {
                        let mut peak = 0;
                        let mut failures = 0;
                        for i in (job..dirs.len()).step_by(jobs) {
                            let output = crate::self_command()?
                                .arg("verify")
                                .arg(&dirs[i])
                                .arg(model)
                                .output()?;
                            let value: Value = serde_json::from_slice(&output.stdout)
                                .with_context(|| {
                                    format!(
                                        "verify child: {}",
                                        String::from_utf8_lossy(&output.stderr)
                                    )
                                })?;
                            peak = peak.max(value["worker_hwm_kib"].as_u64().unwrap_or(0));
                            if value["pass"] != true {
                                failures += 1;
                            }
                            if job == 0 && i % (jobs * 100) == 0 {
                                eprintln!(
                                    "verify about {}/{} elapsed={:.1}s",
                                    i + 1,
                                    dirs.len(),
                                    start.elapsed().as_secs_f64()
                                );
                            }
                        }
                        Ok((peak, failures))
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .map_err(|_| anyhow::anyhow!("verification worker panicked"))?
                })
                .collect::<Result<Vec<_>>>()
        })?;
        for (peak, failed) in results {
            peak_worker = peak_worker.max(peak);
            failures += failed;
        }
    }
    for dir in &dirs {
        match method {
            "sha256" => {
                black_box(digest(dir, false)?);
            }
            "blake3" => {
                black_box(digest(dir, true)?);
            }
            "identity" => {
                for file in files(dir)? {
                    let metadata = std::fs::metadata(file)?;
                    black_box((metadata.len(), metadata.modified()?));
                }
            }
            "summaries" => {
                let value: Value = serde_json::from_slice(&std::fs::read(dir.join(SUMMARY))?)?;
                black_box(value);
            }
            "verify" => {}
            _ => bail!("unknown phase {method}"),
        }
    }
    let total = start.elapsed().as_secs_f64();
    Ok(
        json!({"method":method,"n":dirs.len(),"total_s":total,"ms_per_item":total*1000./dirs.len() as f64,"rss_kib":rss("VmRSS:"),"hwm_kib":rss("VmHWM:"),"peak_worker_hwm_kib":peak_worker,"failed_packages":failures,"verification_jobs":if method == "verify" {jobs} else {0},"worker_rss_upper_bound_kib":peak_worker*jobs as u64}),
    )
}

fn copy_tree(source: &Path, target: &Path, hardlinks: bool) -> Result<()> {
    std::fs::create_dir_all(target)?;
    for file in files(source)? {
        let dest = target.join(file.strip_prefix(source)?);
        std::fs::create_dir_all(dest.parent().context("copy parent")?)?;
        if hardlinks {
            std::fs::hard_link(file, dest)?;
        } else {
            std::fs::copy(file, dest)?;
        }
    }
    std::fs::copy(source.join(SUMMARY), target.join(SUMMARY))?;
    Ok(())
}

pub fn many(source: &Path, n: usize, hardlinks: bool) -> Result<()> {
    let temp = tempfile::tempdir()?;
    let real = [source.join("surfaces/zone"), source.join("surfaces/pico")];
    let model = source.join("fixtures/model.json");
    let text = std::fs::read_to_string(&model)?;
    // Derived summaries are created before timing startup phases.
    for package in &real {
        conformance::verify(package, &text, true)?;
    }
    let stage = tempfile::tempdir()?;
    for (i, package) in real.iter().enumerate() {
        copy_tree(package, &stage.path().join(i.to_string()), false)?;
    }
    for i in 0..n {
        copy_tree(
            &stage.path().join((i % 2).to_string()),
            &temp.path().join(format!("surface-{i:05}")),
            hardlinks,
        )?;
    }
    for method in ["sha256", "blake3", "identity", "summaries", "verify"] {
        let status = crate::self_command()?
            .args(["phase", method])
            .arg(temp.path())
            .arg(&model)
            .status()?;
        if !status.success() {
            bail!("phase {method} failed: {status}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digest_changes_with_bytes_not_summary() -> Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("main.slint"), "one")?;
        let before = digest(dir.path(), false)?;
        std::fs::write(dir.path().join(SUMMARY), "derived")?;
        assert_eq!(before, digest(dir.path(), false)?);
        std::fs::write(dir.path().join("main.slint"), "two")?;
        assert_ne!(before, digest(dir.path(), false)?);
        Ok(())
    }
}
