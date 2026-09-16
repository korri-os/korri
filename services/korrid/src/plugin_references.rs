//! Named native artifacts belong to the package that declares their file key.
//! Nix closure references carry native dependencies; Korri does not maintain a
//! second package-dependency graph.
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct RunnerReference {
    id: String,
    program: Option<String>,
    core: Option<String>,
}

#[derive(Deserialize)]
struct References {
    #[serde(default)]
    runners: BTreeMap<String, RunnerReference>,
}

pub fn validate_files(
    declaration: serde_json::Value,
    files: &BTreeMap<String, std::path::PathBuf>,
) -> Result<(), String> {
    let references: References = serde_json::from_value(declaration).map_err(|e| e.to_string())?;
    for runner in references.runners.values() {
        for key in [runner.program.as_ref(), runner.core.as_ref()]
            .into_iter()
            .flatten()
        {
            if !files.contains_key(key) {
                return Err(format!(
                    "runner {} requires file {key} in its own package",
                    runner.id
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn file_keys_belong_to_the_contributions_own_package() {
        let data = json!({"runners":{"ra":{"id":"@korri:ra/ra", "program":"ra", "core":"core"}}});
        let files = BTreeMap::from([("ra".into(), std::path::PathBuf::from("/program"))]);
        assert!(validate_files(data.clone(), &files)
            .unwrap_err()
            .contains("core"));
        let mut files = files;
        files.insert("core".into(), "/core".into());
        validate_files(data, &files).unwrap();
    }
}
