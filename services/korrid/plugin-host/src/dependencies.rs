use std::{collections::BTreeMap, path::PathBuf};

/// A projection of existing selections and each selected manifest's `requires`.
/// Store closure membership alone is never installation or approval.
#[derive(Clone, Debug)]
pub struct SelectedPackage {
    pub id: String,
    pub package: PathBuf,
    pub enabled: bool,
    pub requires: Vec<PathBuf>,
}

/// Validate the complete post-operation selection before making any effects.
/// Disabled packages can be installed in dependency order. Activation requires
/// every exact dependency to be active; replacement/removal cannot strand even
/// a disabled installed package on an unselected build.
pub fn validate_selection(packages: &[SelectedPackage]) -> Result<(), String> {
    let by_path: BTreeMap<_, _> = packages.iter().map(|p| (&p.package, p)).collect();
    for package in packages {
        for required in &package.requires {
            let dependency = by_path.get(required).ok_or_else(|| {
                format!(
                    "{} requires exact installed plugin {}; inspect and approve that build first",
                    package.id,
                    required.display()
                )
            })?;
            if package.enabled && !dependency.enabled {
                return Err(format!(
                    "enabled plugin {} requires enabled plugin {} ({})",
                    package.id,
                    dependency.id,
                    required.display()
                ));
            }
        }
    }
    for (_, dependencies) in dependency_order(packages.iter().map(|p| p.package.clone()), |path| {
        Ok(by_path[path].requires.clone())
    }) {
        dependencies?;
    }
    Ok(())
}

/// Load only the graph reachable from the roots, once per exact package. Return
/// dependencies before dependents, with load failures and cycles kept local to
/// their nodes. Callers propagate failed outcomes along the returned edges.
/// Both discovery and ordering are iterative: deep chains cannot exhaust the
/// stack, and shared or cyclic edges cannot cause repeated work.
pub fn dependency_order(
    roots: impl IntoIterator<Item = PathBuf>,
    mut load: impl FnMut(&PathBuf) -> Result<Vec<PathBuf>, String>,
) -> Vec<(PathBuf, Result<Vec<PathBuf>, String>)> {
    let mut graph = BTreeMap::new();
    let mut pending: Vec<_> = roots.into_iter().collect();
    while let Some(path) = pending.pop() {
        if graph.contains_key(&path) {
            continue;
        }
        let dependencies = load(&path).map(|mut paths| {
            paths.sort();
            paths.dedup();
            pending.extend(paths.iter().cloned());
            paths
        });
        graph.insert(path, dependencies);
    }

    let mut counts = BTreeMap::new();
    let mut dependents: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    let mut ready = std::collections::BTreeSet::new();
    for (path, dependencies) in &graph {
        let dependencies = dependencies.as_ref().map(Vec::as_slice).unwrap_or(&[]);
        counts.insert(path.clone(), dependencies.len());
        if dependencies.is_empty() {
            ready.insert(path.clone());
        }
        for dependency in dependencies {
            dependents
                .entry(dependency.clone())
                .or_default()
                .push(path.clone());
        }
    }
    let mut ordered = Vec::with_capacity(graph.len());
    while let Some(path) = ready.pop_first() {
        ordered.push((path.clone(), graph.remove(&path).unwrap()));
        if let Some(parents) = dependents.remove(&path) {
            for parent in parents {
                let count = counts.get_mut(&parent).unwrap();
                *count -= 1;
                if *count == 0 {
                    ready.insert(parent);
                }
            }
        }
    }
    // Remaining nodes are in a cycle or require one. None may be activated.
    ordered.extend(graph.into_keys().map(|path| {
        let error = format!("plugin {} has a dependency cycle", path.display());
        (path, Err(error))
    }));
    ordered
}
