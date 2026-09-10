use korrid::config::resolver::{
    resolve_session_controls, ActiveRouteContext, RouteContribution, RoutePlatform,
    SessionExecutorAvailability,
};
use korrid::plugin::{
    load_plugin_source, PluginRegistry, SessionControlExecutor, SessionControlOwnerKind,
};

const RETROARCH_PLUGIN: &str = include_str!("../../../plugins/retroarch/plugin.ts");

fn controls(enabled: bool) -> Vec<korrid::plugin::SessionControlRecord> {
    let plugin =
        load_plugin_source("@korri", RETROARCH_PLUGIN).expect("canonical RetroArch declaration");
    let registry = PluginRegistry::new(
        vec![plugin],
        enabled.then(|| "@korri:retroarch".to_owned()).into_iter(),
    )
    .expect("RetroArch registry");
    resolve_session_controls(
        &registry,
        &ActiveRouteContext {
            platform: RoutePlatform::Android,
            contributors: vec![RouteContribution {
                kind: SessionControlOwnerKind::Launcher,
                id: "@korri:retroarch/retroarch".into(),
            }],
            executor_availability: SessionExecutorAvailability::from_available([
                SessionControlExecutor::RetroarchControl,
            ]),
        },
    )
}

#[test]
fn canonical_retroarch_declares_ordered_menu_and_truthful_quit_controls() {
    let controls = controls(true);
    assert_eq!(
        controls
            .iter()
            .map(|control| (
                control.id.as_str(),
                control.label.as_str(),
                control.order,
                control.destructive,
                control.dismiss_on_success,
            ))
            .collect::<Vec<_>>(),
        [
            (
                "@korri:retroarch/open-menu",
                "Open RetroArch menu",
                0,
                false,
                true,
            ),
            ("@korri:retroarch/quit", "Quit game", 1, true, true,),
        ]
    );
}

#[test]
fn retroarch_controls_require_enablement_route_ownership_and_live_executor() {
    assert!(controls(false).is_empty());

    let plugin =
        load_plugin_source("@korri", RETROARCH_PLUGIN).expect("canonical RetroArch declaration");
    let registry = PluginRegistry::new(vec![plugin], vec!["@korri:retroarch".to_owned()])
        .expect("RetroArch registry");
    for context in [
        ActiveRouteContext {
            platform: RoutePlatform::Android,
            contributors: vec![],
            executor_availability: SessionExecutorAvailability::from_available([
                SessionControlExecutor::RetroarchControl,
            ]),
        },
        ActiveRouteContext {
            platform: RoutePlatform::Android,
            contributors: vec![RouteContribution {
                kind: SessionControlOwnerKind::Launcher,
                id: "@korri:android-app/android-app".into(),
            }],
            executor_availability: SessionExecutorAvailability::from_available([
                SessionControlExecutor::RetroarchControl,
            ]),
        },
        ActiveRouteContext {
            platform: RoutePlatform::Android,
            contributors: vec![RouteContribution {
                kind: SessionControlOwnerKind::Transport,
                id: "@korri:moonlight/moonlight".into(),
            }],
            executor_availability: SessionExecutorAvailability::from_available([
                SessionControlExecutor::RetroarchControl,
            ]),
        },
        ActiveRouteContext {
            platform: RoutePlatform::Android,
            contributors: vec![RouteContribution {
                kind: SessionControlOwnerKind::Launcher,
                id: "@korri:retroarch/retroarch".into(),
            }],
            executor_availability: SessionExecutorAvailability::from_available([]),
        },
    ] {
        assert!(resolve_session_controls(&registry, &context).is_empty());
    }
}
