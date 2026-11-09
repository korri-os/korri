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
        self.calls.lock().unwrap().push(format!("start {}", plan.android.package));
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
        self.calls.lock().unwrap().push(format!("end {}", session.package));
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
        vec!["korri", "korri", "resume com.example.game", "end com.example.game"]
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
