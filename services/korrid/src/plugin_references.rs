//! Native references use the brief's kind/program/launcher fields. Android
//! app/path records remain platform declarations, not native launcher aliases.
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf};

/// Existing approved package identity, exact manifest pins and evaluated data.
/// Used for installed and enabled selections; not a persisted representation.
#[derive(Clone)]
pub struct PackageDeclaration {
    pub id: String,
    pub package: PathBuf,
    pub requires: Vec<PathBuf>,
    pub declaration: serde_json::Value,
}

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
/// algorithm. Cross-package references require a direct pin to the selected
/// package, not merely its presence or a transitive dependency.
pub fn validate(declarations: impl IntoIterator<Item = PackageDeclaration>) -> Result<(), String> {
    let mut launchers = BTreeMap::new();
    let mut runtimes = Vec::new();
    let mut packages = BTreeMap::new();
    for declaration in declarations {
        let owner = declaration.id;
        if packages
            .insert(owner.clone(), (declaration.package, declaration.requires))
            .is_some()
        {
            return Err(format!("duplicate package {owner}"));
        }
        let references: References =
            serde_json::from_value(declaration.declaration).map_err(|e| e.to_string())?;
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
            if launchers
                .insert(launcher.id.clone(), (owner.clone(), launcher))
                .is_some()
            {
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
            runtimes.push((owner.clone(), runtime));
        }
    }
    let validate_dependency = |owner: &str, target_owner: &str, reference: &str| {
        let (target_path, _) = &packages[target_owner];
        if owner != target_owner && !packages[owner].1.contains(target_path) {
            return Err(format!(
                "{owner} reference {reference} requires exact manifest dependency {}",
                target_path.display()
            ));
        }
        Ok(())
    };
    for (owner, launcher) in launchers.values() {
        if let Some(kind) = &launcher.kind {
            let (target_owner, _) = launchers
                .get(kind)
                .filter(|(_, record)| record.kind.as_ref() == Some(kind))
                .ok_or_else(|| {
                    format!("launcher {} requires unavailable kind {kind}", launcher.id)
                })?;
            validate_dependency(owner, target_owner, kind)?;
        }
    }
    for (owner, runtime) in runtimes {
        if let Some(launcher) = runtime.launcher {
            let (target_owner, _) = launchers
                .get(&launcher)
                .filter(|(_, record)| record.kind.is_some())
                .ok_or_else(|| {
                    format!(
                        "runtime {} requires unavailable launcher {launcher}",
                        runtime.id
                    )
                })?;
            validate_dependency(&owner, target_owner, &launcher)?;
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
        let runtime = PackageDeclaration {
            id: "@simon:mgba".into(),
            package: "/mgba".into(),
            requires: vec!["/retroarch".into()],
            declaration: json!({"runtimes":{"mgba":{"id":"@simon:mgba/mgba", "launcher":"@korri:retroarch/retroarch"}}}),
        };
        let renamed = PackageDeclaration {
            id: "@korri:retroarch".into(),
            package: "/retroarch".into(),
            requires: vec![],
            declaration: json!({"launchers":{"changed":{"id":"@korri:retroarch/changed", "kind":"@korri:retroarch/changed", "program":"retroarch"}}}),
        };
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
        assert!(validate([PackageDeclaration {
            id: "@korri:ra".into(),
            package: "/ra".into(),
            requires: vec![],
            declaration: data,
        }])
        .unwrap_err()
        .contains("@korri:ra/instance"));
    }
}
