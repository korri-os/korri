//! Exact-output graphs, restored from f3aa66d91. Discovery is bounded and
//! iterative; neither hostile depth nor cycles can recurse through the host.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub const MAX_PLUGINS: usize = 128;
pub const MAX_EDGES: usize = 1024;

pub fn dependency_order(
    roots: impl IntoIterator<Item = PathBuf>,
    mut load: impl FnMut(&PathBuf) -> Result<Vec<PathBuf>, String>,
) -> Result<Vec<PathBuf>, String> {
    let mut graph = BTreeMap::new();
    let mut pending: Vec<_> = roots.into_iter().collect();
    let mut edges = 0;
    while let Some(path) = pending.pop() {
        if graph.contains_key(&path) {
            continue;
        }
        if graph.len() >= MAX_PLUGINS {
            return Err("plugin dependency graph exceeds 128 plugins".into());
        }
        let mut requires = load(&path)?;
        requires.sort();
        requires.dedup();
        edges += requires.len();
        if edges > MAX_EDGES {
            return Err("plugin dependency graph exceeds 1024 edges".into());
        }
        pending.extend(requires.iter().cloned());
        graph.insert(path, requires);
    }
    let mut counts = BTreeMap::new();
    let mut parents: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    let mut ready = BTreeSet::new();
    for (path, requires) in &graph {
        counts.insert(path.clone(), requires.len());
        if requires.is_empty() {
            ready.insert(path.clone());
        }
        for dependency in requires {
            parents
                .entry(dependency.clone())
                .or_default()
                .push(path.clone());
        }
    }
    let mut order = Vec::new();
    while let Some(path) = ready.pop_first() {
        order.push(path.clone());
        if let Some(dependents) = parents.remove(&path) {
            for parent in dependents {
                let count = counts.get_mut(&parent).unwrap();
                *count -= 1;
                if *count == 0 {
                    ready.insert(parent);
                }
            }
        }
    }
    if order.len() != graph.len() {
        return Err("plugin dependency cycle".into());
    }
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_dependencies_are_loaded_once_before_parents() {
        let mut loaded = Vec::new();
        let order = dependency_order(["pack".into()], |p| {
            loaded.push(p.clone());
            Ok(match p.to_str().unwrap() {
                "pack" => vec!["a".into(), "b".into()],
                "a" | "b" => vec!["runner".into()],
                _ => vec![],
            })
        })
        .unwrap();
        assert_eq!(order, ["runner", "a", "b", "pack"].map(PathBuf::from));
        assert_eq!(loaded.len(), 4);
    }
    #[test]
    fn cycles_and_deep_or_wide_graphs_are_refused() {
        assert!(dependency_order(["cycle".into()], |p| Ok(vec![p.clone()]))
            .unwrap_err()
            .contains("cycle"));
        assert!(dependency_order(["0".into()], |p| Ok(vec![PathBuf::from(
            (p.to_str().unwrap().parse::<usize>().unwrap() + 1).to_string()
        )]))
        .unwrap_err()
        .contains("128"));
        assert!(dependency_order(["root".into()], |_| Ok((0..2048)
            .map(|i| PathBuf::from(i.to_string()))
            .collect()))
        .unwrap_err()
        .contains("1024"));
    }
}
