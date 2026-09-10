use korri_plugin_host::dependencies::{dependency_order, validate_selection, SelectedPackage};
use std::{collections::BTreeMap, path::PathBuf};

fn selected(id: &str, path: &str, enabled: bool, requires: &[&str]) -> SelectedPackage {
    SelectedPackage {
        id: id.into(),
        package: PathBuf::from(path),
        enabled,
        requires: requires.iter().map(PathBuf::from).collect(),
    }
}

#[test]
fn an_exact_requirement_is_not_satisfied_by_a_different_build_or_store_presence() {
    let launcher = selected("@korri:retroarch", "/launcher-a", true, &[]);
    let core = selected("@korri:mgba", "/core", true, &["/launcher-a"]);
    validate_selection(&[launcher.clone(), core.clone()]).unwrap();
    assert!(validate_selection(std::slice::from_ref(&core))
        .unwrap_err()
        .contains("/launcher-a"));
    let other = selected("@korri:retroarch", "/launcher-b", true, &[]);
    assert!(validate_selection(&[other, core])
        .unwrap_err()
        .contains("@korri:mgba"));
}

#[test]
fn rollback_keeps_exact_closure_pins_even_for_disabled_installed_dependents() {
    use korri_plugin_host::selection::Receipt;
    let first: Receipt = serde_json::from_str(include_str!("fixtures/selection.json")).unwrap();
    let current = first
        .select(
            "/nix/store/11111111111111111111111111111111-new".into(),
            first.provenance.clone(),
            "new-approval".into(),
        )
        .unwrap();
    let restored = current.rollback().unwrap();
    for enabled in [false, true] {
        let dependency = selected(&current.id, current.package.to_str().unwrap(), enabled, &[]);
        let dependent = selected(
            "@korri:mgba",
            "/core",
            enabled,
            &[current.package.to_str().unwrap()],
        );
        validate_selection(&[dependency.clone(), dependent.clone()]).unwrap();
        let candidate = SelectedPackage {
            package: restored.package.clone(),
            ..dependency
        };
        let error = validate_selection(&[candidate, dependent]).unwrap_err();
        assert!(error.contains("@korri:mgba"), "{error}");
        assert!(error.contains(current.package.to_str().unwrap()), "{error}");
    }
}

#[test]
fn removing_or_renaming_a_launcher_or_kind_is_refused_without_manifest_requires() {
    use korri_plugin_host::plugin_references;
    use serde_json::json;
    // Native field names and IDs come from plugins/{retroarch,mgba}/plugin.ts.
    let kind = (
        "@korri:retroarch".into(),
        json!({"launchers":{"retroarch":{"id":"@korri:retroarch/retroarch", "kind":"@korri:retroarch/retroarch", "program":"retroarch"}}}),
    );
    let launcher = (
        "@test:launcher".into(),
        json!({"launchers":{"retroarch":{"id":"@test:launcher/retroarch", "kind":"@korri:retroarch/retroarch", "program":"retroarch"}}}),
    );
    let runtime = (
        "@korri:mgba".into(),
        json!({"runtimes":{"mgba":{"id":"@korri:mgba/mgba", "launcher":"@test:launcher/retroarch", "path":"mgba"}}}),
    );
    plugin_references::validate([kind.clone(), launcher.clone(), runtime.clone()]).unwrap();
    let error = plugin_references::validate([launcher.clone(), runtime.clone()]).unwrap_err();
    assert!(error.contains("@korri:retroarch/retroarch"), "{error}");
    let error = plugin_references::validate([kind.clone(), runtime.clone()]).unwrap_err();
    assert!(error.contains("@test:launcher/retroarch"), "{error}");
    let renamed = (
        "@test:launcher".into(),
        json!({"launchers":{"renamed":{"id":"@test:launcher/renamed", "kind":"@korri:retroarch/retroarch", "program":"retroarch"}}}),
    );
    assert!(plugin_references::validate([kind, renamed, runtime])
        .unwrap_err()
        .contains("@test:launcher/retroarch"));
}

#[test]
fn enabling_a_core_requires_enabled_dependencies_but_install_can_prepare_disabled_packages() {
    let launcher = selected("@korri:retroarch", "/launcher", false, &[]);
    let core = selected("@korri:mgba", "/core", false, &["/launcher"]);
    validate_selection(&[launcher.clone(), core.clone()]).unwrap();
    let error = validate_selection(&[
        launcher,
        SelectedPackage {
            enabled: true,
            ..core
        },
    ])
    .unwrap_err();
    assert!(error.contains("@korri:retroarch"), "{error}");
    assert!(error.contains("@korri:mgba"), "{error}");
}

#[test]
fn all_required_plugins_are_checked_not_only_the_first_or_the_launcher_kind() {
    let a = selected("@vendor:a", "/a", true, &[]);
    let b = selected("@vendor:b", "/b", true, &[]);
    let core = selected("@other:core", "/core", true, &["/a", "/b"]);
    validate_selection(&[a.clone(), b, core.clone()]).unwrap();
    assert!(validate_selection(&[a, core]).unwrap_err().contains("/b"));
}

#[test]
fn pending_dependencies_are_ordered_before_dependents_regardless_of_directory_order() {
    for roots in [["/a", "/b"], ["/b", "/a"]] {
        let order = dependency_order(roots.map(PathBuf::from), |path| {
            Ok(if path == &PathBuf::from("/a") {
                vec!["/b".into()]
            } else {
                vec![]
            })
        });
        assert_eq!(
            order,
            vec![
                ("/b".into(), Ok(vec![])),
                ("/a".into(), Ok(vec!["/b".into()]))
            ]
        );
    }
}

#[test]
fn rooted_lookup_ignores_unrelated_corruption_but_reports_required_corruption() {
    let mut graph = BTreeMap::from([
        (PathBuf::from("/a"), Ok(vec![PathBuf::from("/b")])),
        (PathBuf::from("/b"), Ok(vec![])),
        (
            PathBuf::from("/c"),
            Err("invalid plugin receipt".to_owned()),
        ),
    ]);
    let order = dependency_order(["/a".into()], |path| graph[path].clone());
    assert_eq!(order.len(), 2);
    assert!(order.iter().all(|(_, result)| result.is_ok()));
    // Boot also includes C as an independent root; its error does not erase
    // the healthy A -> B ordering.
    let order = dependency_order(graph.keys().cloned(), |path| graph[path].clone());
    assert_eq!(order.len(), 3);
    assert_eq!(
        order.iter().filter(|(_, result)| result.is_err()).count(),
        1
    );
    graph.insert("/b".into(), Err("invalid plugin receipt".into()));
    let order = dependency_order(["/a".into()], |path| graph[path].clone());
    assert_eq!(
        order[0],
        ("/b".into(), Err("invalid plugin receipt".into()))
    );
    assert_eq!(order[1], ("/a".into(), Ok(vec!["/b".into()])));
}

#[test]
fn cycles_and_their_dependents_fail_without_blocking_an_independent_root() {
    let graph = BTreeMap::from([
        (PathBuf::from("/a"), vec![PathBuf::from("/b")]),
        (PathBuf::from("/b"), vec![PathBuf::from("/a")]),
        (PathBuf::from("/c"), vec![PathBuf::from("/a")]),
        (PathBuf::from("/healthy"), vec![]),
    ]);
    let order = dependency_order(graph.keys().cloned(), |path| Ok(graph[path].clone()));
    assert_eq!(order[0], ("/healthy".into(), Ok(vec![])));
    assert_eq!(order.len(), 4);
    assert!(order[1..]
        .iter()
        .all(|(_, result)| result.as_ref().unwrap_err().contains("cycle")));
    for enabled in [false, true] {
        assert!(validate_selection(&[
            selected("@vendor:a", "/a", enabled, &["/b"]),
            selected("@vendor:b", "/b", enabled, &["/a"]),
        ])
        .unwrap_err()
        .contains("cycle"));
        assert!(
            validate_selection(&[selected("@vendor:a", "/a", enabled, &["/a"])])
                .unwrap_err()
                .contains("cycle")
        );
    }
}

#[test]
fn all_four_node_dags_are_ordered_once_with_shared_and_duplicate_requirements() {
    // All 64 subsets of the six possible edges in a four-node DAG, each
    // with all 24 directory enumeration orders. IDs intentionally run in the
    // opposite order from their dependency topology.
    let nodes: Vec<PathBuf> = ["/d", "/c", "/b", "/a"].map(PathBuf::from).into();
    let edges = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];
    for mask in 0..64 {
        let mut graph: BTreeMap<_, Vec<PathBuf>> =
            nodes.iter().map(|n| (n.clone(), vec![])).collect();
        for (bit, (parent, child)) in edges.iter().enumerate() {
            if mask & (1 << bit) != 0 {
                graph
                    .get_mut(&nodes[*parent])
                    .unwrap()
                    .extend([nodes[*child].clone(), nodes[*child].clone()]);
            }
        }
        for a in 0..4 {
            for b in 0..4 {
                for c in 0..4 {
                    for d in 0..4 {
                        let indices = [a, b, c, d];
                        if indices
                            .iter()
                            .copied()
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                            != 4
                        {
                            continue;
                        }
                        let mut loads = BTreeMap::new();
                        let order = dependency_order(indices.map(|i| nodes[i].clone()), |path| {
                            *loads.entry(path.clone()).or_insert(0) += 1;
                            Ok(graph[path].clone())
                        });
                        assert_eq!(order.len(), 4);
                        assert!(loads.values().all(|count| *count == 1));
                        let positions: BTreeMap<_, _> = order
                            .iter()
                            .enumerate()
                            .map(|(index, (path, _))| (path, index))
                            .collect();
                        for (path, dependencies) in &order {
                            for dependency in dependencies.as_ref().unwrap() {
                                assert!(positions[dependency] < positions[path]);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn a_deep_chain_uses_bounded_work_without_recursive_stack_growth() {
    let mut loads = 0;
    let order = dependency_order(["0".into()], |path| {
        loads += 1;
        let index: usize = path.to_str().unwrap().parse().unwrap();
        Ok(if index < 9999 {
            vec![(index + 1).to_string().into()]
        } else {
            vec![]
        })
    });
    assert_eq!(loads, 10000);
    assert_eq!(order.len(), loads);
    assert_eq!(order.first().unwrap().0, PathBuf::from("9999"));
    assert_eq!(order.last().unwrap().0, PathBuf::from("0"));
}
