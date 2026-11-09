use super::*;

#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<String>>,
}
impl AppLauncher for Fake {
    fn is_installed(&self, package: &str) -> bool {
        package == "com.example.game"
    }
    fn start(&self, plan: &AndroidLaunchOutput, _: Option<&str>) -> Result<(), String> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("start {}", plan.android.package));
        Ok(())
    }
    fn resume(&self, package: &str) -> Result<(), String> {
        self.calls.lock().unwrap().push(format!("resume {package}"));
        Ok(())
    }
    fn show_korri(&self) -> Result<(), String> {
        self.calls.lock().unwrap().push("korri".into());
        Ok(())
    }
    fn end_session(&self, session: &LocalSession) -> Result<(), String> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("end {}", session.package));
        Ok(())
    }
}

fn live(fake: Arc<Fake>) -> AndroidLocalSessions {
    let sessions = AndroidLocalSessions::new(fake);
    *sessions.active.lock().unwrap() = Some(LocalSession {
        launch_id: "a".repeat(32),
        game_id: "tmnt".into(),
        title: Some("TMNT".into()),
        package: "com.example.game".into(),
        phase: Phase::Playing,
        save_locations: Vec::new(),
    });
    sessions
}

#[test]
fn leave_resume_and_end_follow_the_live_session() {
    let fake = Arc::new(Fake::default());
    let sessions = live(fake.clone());
    let id = "a".repeat(32);
    assert!(sessions.leave(Some("b")).is_err(), "only the exact launch");
    let (left, changed) = sessions.leave(Some(&id)).unwrap();
    assert_eq!((left.phase, changed), (Phase::Left, true));
    assert!(!sessions.leave(None).unwrap().1, "already left");
    assert_eq!(sessions.resume(None).unwrap().0.phase, Phase::Playing);
    sessions.end(Some(&id)).unwrap();
    assert!(sessions.active().is_none());
    assert_eq!(
        *fake.calls.lock().unwrap(),
        vec![
            "korri",
            "korri",
            "resume com.example.game",
            "end com.example.game"
        ]
    );
}

#[test]
fn usage_events_mark_left_and_a_closed_app_ends_the_session() {
    let sessions = live(Arc::new(Fake::default()));
    sessions.app_left("com.other.app", true);
    assert!(sessions.active().is_some(), "another app is not this game");
    sessions.app_left("com.example.game", false);
    assert_eq!(sessions.active().unwrap().phase, Phase::Left);
    sessions.app_left("com.example.game", true);
    assert!(sessions.active().is_none());
}

/// Runs the real `@korri:android-app` package (korri-os/plugins) through the
/// Android admission, discovery and launch paths. Set KORRI_ANDROID_APP_PACKAGE
/// to its built package folder; skipped when unset.
#[test]
fn the_real_android_app_plugin_discovers_and_launches_on_android() {
    let Ok(package) = std::env::var("KORRI_ANDROID_APP_PACKAGE") else {
        return;
    };
    let package = std::path::PathBuf::from(package);
    let source = crate::script::source::SourceSnapshot::package_plugin(
        &package,
        "plugin.ts",
        &["plugin.ts".to_owned()],
    )
    .unwrap();
    let json = crate::script::eval_plugin_snapshot(&source).unwrap();
    let registry = |platform| {
        let plugin = crate::plugin::decode_plugin_declaration_on("@korri", &json, platform).unwrap();
        crate::plugin::PluginRegistry::new(vec![plugin], ["@korri:android-app".to_owned()]).unwrap()
    };
    assert!(registry("aarch64-android").runners().contains_key("@korri:android-app/android-app"));
    assert!(registry("aarch64-linux").runners().is_empty(), "no Android runner on Linux");

    let input = serde_json::json!({"apps": [
        {"packageName": "com.example.game", "label": "Game", "isGame": true, "added": false},
        {"packageName": "com.android.settings", "label": "Settings", "isGame": false, "added": false},
        {"packageName": "com.example.mine", "label": "Mine", "isGame": false, "added": true}
    ]});
    let output = crate::script::call_plugin_operation_snapshot(&source, "discovery.discover", &input.to_string()).unwrap();
    let observations: Vec<crate::discovery::provider::ProviderRefReleaseObservation> =
        serde_json::from_str(&output).unwrap();
    let refs: Vec<_> = observations.iter().map(|o| o.target.provider_ref.as_str()).collect();
    assert_eq!(refs, vec!["com.example.game", "com.example.mine"]);

    let launch: PluginLaunchInput = serde_json::from_value(serde_json::json!({
        "runnerId": "@korri:android-app/android-app", "program": "", "ref": "com.example.game",
        "accountRoot": "/a", "saveRoot": "/a/s", "cacheRoot": "/c", "files": {}, "artifacts": {},
        "androidPackage": "com.example.game"
    }))
    .unwrap();
    let plan = plugin_launch::evaluate_android(&source, &launch, &["com.example.game".to_owned()]).unwrap();
    assert_eq!(plan.android.package, "com.example.game");
    assert!(plugin_launch::evaluate_android(&source, &launch, &["com.other".to_owned()]).is_err());
}
