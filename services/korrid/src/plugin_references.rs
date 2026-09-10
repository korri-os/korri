//! Native references use the brief's kind/program/launcher fields. Android
//! app/path records remain platform declarations, not native launcher aliases.
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct LauncherReference {
    id: String,
    kind: Option<String>,
    program: Option<String>,
}
#[derive(Deserialize)]
struct RuntimeReference {
    id: String,
    launcher: Option<String>,
    path: Option<String>,
}
#[derive(Deserialize)]
struct References {
    #[serde(default)]
    launchers: BTreeMap<String, LauncherReference>,
    #[serde(default)]
    runtimes: BTreeMap<String, RuntimeReference>,
}

pub fn validate_files(
    declaration: serde_json::Value,
    files: &BTreeMap<String, std::path::PathBuf>,
) -> Result<(), String> {
    let references: References = serde_json::from_value(declaration).map_err(|e| e.to_string())?;
    for launcher in references
        .launchers
        .values()
        .filter(|launcher| launcher.kind.is_some())
    {
        let key = launcher
            .program
            .as_ref()
            .ok_or_else(|| format!("launcher {} has no program", launcher.id))?;
        if !files.contains_key(key) {
            return Err(format!(
                "launcher {} requires file {key} in its own package",
                launcher.id
            ));
        }
    }
    for runtime in references
        .runtimes
        .values()
        .filter(|runtime| runtime.launcher.is_some())
    {
        let key = runtime
            .path
            .as_ref()
            .ok_or_else(|| format!("runtime {} has no path", runtime.id))?;
        if !files.contains_key(key) {
            return Err(format!(
                "runtime {} requires file {key} in its own package",
                runtime.id
            ));
        }
    }
    Ok(())
}

/// Validate both all-installed and enabled-only selections with the same
/// algorithm. Exact manifest dependencies remain separately mandatory.
pub fn validate(
    declarations: impl IntoIterator<Item = (String, serde_json::Value)>,
) -> Result<(), String> {
    let mut launchers = BTreeMap::new();
    let mut runtimes = Vec::new();
    for (owner, value) in declarations {
        let references: References = serde_json::from_value(value).map_err(|e| e.to_string())?;
        for (local, launcher) in references.launchers {
            if launcher.id != format!("{owner}/{local}") {
                return Err(format!(
                    "launcher {} must belong to {owner}/{local}",
                    launcher.id
                ));
            }
            if launcher.kind.is_some() != launcher.program.is_some() {
                return Err(format!(
                    "launcher {} requires kind and program",
                    launcher.id
                ));
            }
            if launchers.insert(launcher.id.clone(), launcher).is_some() {
                return Err("duplicate launcher".into());
            }
        }
        for (local, runtime) in references.runtimes {
            if runtime.id != format!("{owner}/{local}") {
                return Err(format!(
                    "runtime {} must belong to {owner}/{local}",
                    runtime.id
                ));
            }
            runtimes.push(runtime);
        }
    }
    for launcher in launchers.values() {
        if let Some(kind) = &launcher.kind {
            if !launchers
                .get(kind)
                .is_some_and(|record| record.kind.as_ref() == Some(kind))
            {
                return Err(format!(
                    "launcher {} requires unavailable kind {kind}",
                    launcher.id
                ));
            }
        }
    }
    for runtime in runtimes {
        if let Some(launcher) = runtime.launcher {
            if !launchers
                .get(&launcher)
                .is_some_and(|record| record.kind.is_some())
            {
                return Err(format!(
                    "runtime {} requires unavailable launcher {launcher}",
                    runtime.id
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
        let data = json!({"launchers":{"ra":{"id":"@korri:ra/ra", "kind":"@korri:ra/ra", "program":"ra"}}, "runtimes":{"core":{"id":"@korri:ra/core", "launcher":"@korri:ra/ra", "path":"core"}}});
        let files = BTreeMap::from([("ra".into(), std::path::PathBuf::from("/program"))]);
        assert!(validate_files(data.clone(), &files)
            .unwrap_err()
            .contains("core"));
        let mut files = files;
        files.insert("core".into(), "/core".into());
        validate_files(data, &files).unwrap();
    }

    #[test]
    fn a_named_export_removal_cannot_hide_behind_an_unchanged_package_dependency() {
        let runtime = (
            "@simon:mgba".into(),
            json!({"runtimes":{"mgba":{"id":"@simon:mgba/mgba", "launcher":"@korri:retroarch/retroarch"}}}),
        );
        let renamed = (
            "@korri:retroarch".into(),
            json!({"launchers":{"changed":{"id":"@korri:retroarch/changed", "kind":"@korri:retroarch/changed", "program":"retroarch"}}}),
        );
        assert!(validate([runtime, renamed])
            .unwrap_err()
            .contains("@korri:retroarch/retroarch"));
    }
    #[test]
    fn instances_can_borrow_only_a_real_kind_not_an_instance_chain() {
        let data = json!({"launchers":{
            "kind":{"id":"@korri:ra/kind", "kind":"@korri:ra/kind", "program":"default"},
            "instance":{"id":"@korri:ra/instance", "kind":"@korri:ra/kind", "program":"other"},
            "chain":{"id":"@korri:ra/chain", "kind":"@korri:ra/instance", "program":"third"}
        }});
        assert!(validate([("@korri:ra".into(), data)])
            .unwrap_err()
            .contains("@korri:ra/instance"));
    }
}
