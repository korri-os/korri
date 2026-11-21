//! The engine host's line protocol (ADR 0083, contract "Commands and events").
//!
//! Grounding: the spike's host v8 (`korri-steam-spike/host/main.rs`) writes
//! one JSON object per line with an `event` name; its commands are one word
//! and an optional app id per line. The QR, list, save and engine-state
//! events are the contract's additions; `engine_map_missing`,
//! `engine_mismatch` and `engine_refused` are the names the parent fixed on
//! 2026-11-18. korrid reads only the fields below and ignores every other
//! event and field, so a host may report more than korrid uses.
use serde::Deserialize;

/// A Steam app id. The engine's `AppId_t` is a `uint32`.
pub type AppId = u32;

/// One command line korrid writes to the host's FIFO.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    Launched(AppId),
    Exited(AppId),
    Sync(AppId),
    Install(AppId),
    Uninstall(AppId),
    Shutdown,
    SignInQr,
    SignOut,
    ListApps,
    Resolve(AppId, Side),
    /// The person's answer to a launch's pending remote operation.
    Answer(AppId, Answer),
}

/// What a person chooses when another device's save has not finished
/// uploading (ADR 0083, owner decision 2026-11-21), as in Valve's client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Answer {
    /// Ask Steam again.
    Retry,
    /// Start with what Steam Cloud holds now: the engine's `ignore pending`.
    PlayAnyway,
}

/// The side a person keeps in a Steam Cloud conflict.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Local,
    Cloud,
}

impl Command {
    /// The line, with its newline. No command carries a value korrid did not
    /// make itself, so no line can carry a second command.
    pub fn line(self) -> String {
        match self {
            Self::Launched(app) => format!("launched {app}\n"),
            Self::Exited(app) => format!("exited {app}\n"),
            Self::Sync(app) => format!("sync {app}\n"),
            Self::Install(app) => format!("install {app}\n"),
            Self::Uninstall(app) => format!("uninstall {app}\n"),
            Self::Shutdown => "shutdown\n".into(),
            Self::SignInQr => "signin-qr\n".into(),
            Self::SignOut => "signout\n".into(),
            Self::ListApps => "list-apps\n".into(),
            Self::Resolve(app, Side::Local) => format!("resolve {app} local\n"),
            Self::Resolve(app, Side::Cloud) => format!("resolve {app} cloud\n"),
            Self::Answer(app, Answer::Retry) => format!("retry {app}\n"),
            Self::Answer(app, Answer::PlayAnyway) => format!("play-anyway {app}\n"),
        }
    }
}

/// One pending remote operation as the host read it. `operation` is the
/// engine's kind (`upload_pending`, `upload_in_progress`,
/// `app_session_active`, `app_session_suspended` or `unknown`); `machine`
/// is the other device's name, when Steam gave one.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PendingOperation {
    pub operation: String,
    #[serde(default)]
    pub machine: Option<String>,
}

/// Where an app in the person's list comes from.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum AppSource {
    Owned,
    Free,
    Family,
}

/// The events korrid acts on. Everything else is `Other`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// The host's map is the public empty one (contract "Engine map").
    EngineMapMissing {},
    /// The engine file is not the build the map was made for.
    EngineMismatch {},
    /// Steam refused the pinned engine at sign-in.
    EngineRefused {
        eresult: i64,
    },
    LogonState {
        logged_on: bool,
        #[serde(default)]
        offline: bool,
    },
    QrCode {
        challenge_url: String,
    },
    QrRefreshed {
        challenge_url: String,
    },
    SignedIn {
        account_name: String,
    },
    SigninFailed {
        reason: String,
    },
    /// Nobody is signed in. `reason` says why the saved login was not used
    /// (no_account_file, no_saved_login, select_failed); it holds no secret.
    SignedOut {
        #[serde(default)]
        reason: Option<String>,
    },
    /// `steam-engine-session` refused to start the game. `reason` is one
    /// word (session_busy, signin_timeout, install_timeout, install_failed,
    /// not_installed_offline, no_launch_entry, game_files_missing);
    /// `message` is the finished text the person sees.
    LaunchRefused {
        reason: String,
        message: String,
    },
    App {
        appid: AppId,
        name: String,
        source: AppSource,
    },
    AppsDone {},
    AppJobStarted {
        appid: AppId,
        kind: String,
    },
    AppProgress {
        appid: AppId,
        bytes_downloaded: u64,
        bytes_to_download: u64,
        #[serde(default)]
        bytes_staged: u64,
        #[serde(default)]
        bytes_to_stage: u64,
    },
    AppJobFinished {
        appid: AppId,
        kind: String,
        result: String,
    },
    SyncFinished {
        appid: AppId,
        kind: String,
        result: String,
    },
    SyncConflict {
        appid: AppId,
        #[serde(default)]
        local_time: u64,
        #[serde(default)]
        remote_time: u64,
    },
    /// The launch sync found another device's pending operation and waits
    /// for `retry` or `play-anyway`.
    CloudPending {
        appid: AppId,
        operations: Vec<PendingOperation>,
    },
    /// The host took the answer; the launch sync runs again.
    CloudPendingAnswered {
        appid: AppId,
    },
    SaveExport {
        appid: AppId,
        archive: String,
    },
    /// The app's Steam Cloud set has no files now.
    SaveExportEmpty {
        appid: AppId,
    },
    Phase {
        phase: String,
    },
    /// After a QR sign-in: whether the engine wrote its login to disk
    /// (`config.vdf`). Comes before `signed_in`.
    LoginSaved {
        confirmed: bool,
    },
    /// The host took a command. On `shutdown` it may answer
    /// `deferred: "login_save"`: it ends once the login is saved (up to 31 s).
    Command {
        command: String,
        #[serde(default)]
        deferred: Option<String>,
    },
    #[serde(other)]
    Other,
}

/// One output line, or `None` when it is not a JSON event. A host line is
/// never logged or echoed: it may name a path in the person's folder.
pub fn parse(line: &str) -> Option<Event> {
    serde_json::from_str(line.trim()).ok()
}

/// Results of `app_job_finished` that mean the job did what it was asked.
pub fn app_job_succeeded(result: &str) -> bool {
    matches!(
        result,
        "installed" | "already_installed" | "uninstalled" | "not_installed"
    )
}

/// Results of `sync_finished` that let a launch go on. A conflict stops it
/// until the person picks a side; `offline` is the contract's offline start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncOutcome {
    Ready,
    Offline,
    Conflict,
    /// The pass did not finish. The launch still goes on, as Valve's client
    /// does after a failed cloud pass; the next session syncs again.
    Incomplete,
}

pub fn sync_outcome(result: &str) -> SyncOutcome {
    match result {
        "downloaded" | "synchronized" | "local_changes" | "cloud_disabled" | "no_sync_state" => {
            SyncOutcome::Ready
        }
        "offline" => SyncOutcome::Offline,
        "conflict" | "conflict_not_resolved" => SyncOutcome::Conflict,
        _ => SyncOutcome::Incomplete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_one_line_each() {
        assert_eq!(Command::Launched(584400).line(), "launched 584400\n");
        assert_eq!(Command::Exited(7).line(), "exited 7\n");
        assert_eq!(Command::Sync(7).line(), "sync 7\n");
        assert_eq!(Command::Install(78000).line(), "install 78000\n");
        assert_eq!(Command::Uninstall(78000).line(), "uninstall 78000\n");
        assert_eq!(Command::Shutdown.line(), "shutdown\n");
        assert_eq!(Command::SignInQr.line(), "signin-qr\n");
        assert_eq!(Command::SignOut.line(), "signout\n");
        assert_eq!(Command::ListApps.line(), "list-apps\n");
        assert_eq!(Command::Resolve(1, Side::Local).line(), "resolve 1 local\n");
        assert_eq!(Command::Resolve(1, Side::Cloud).line(), "resolve 1 cloud\n");
        assert_eq!(Command::Answer(7, Answer::Retry).line(), "retry 7\n");
        assert_eq!(
            Command::Answer(7, Answer::PlayAnyway).line(),
            "play-anyway 7\n"
        );
    }

    /// The host's lines for a launch that met another device's pending
    /// operation (plugins engine-host `pending_event`).
    #[test]
    fn a_pending_operation_and_its_answer_parse() {
        assert_eq!(
            parse(
                r#"{"event":"cloud_pending","appid":584400,"operations":[{"operation":"upload_pending"},{"operation":"app_session_active","machine":"Living room"}]}"#
            ),
            Some(Event::CloudPending {
                appid: 584400,
                operations: vec![
                    PendingOperation {
                        operation: "upload_pending".into(),
                        machine: None
                    },
                    PendingOperation {
                        operation: "app_session_active".into(),
                        machine: Some("Living room".into())
                    },
                ]
            })
        );
        assert_eq!(
            parse(r#"{"event":"cloud_pending_answered","appid":7,"answer":"retry"}"#),
            Some(Event::CloudPendingAnswered { appid: 7 })
        );
    }

    /// Lines copied from the spike host's output (FINDINGS sections 7, 8,
    /// 11) and the contract's new events.
    #[test]
    fn spike_and_contract_events_parse_and_others_are_ignored() {
        assert_eq!(
            parse(
                r#"{"event":"app_progress","appid":78000,"kind":"install","state":1026,"names":["Update Required","Update Started"],"up_to_date":false,"update_info":true,"bytes_downloaded":81160832,"bytes_to_download":81160832,"bytes_staged":97849494,"bytes_to_stage":97849494,"update_result":0,"update_result_name":"No Error","downloading_appid":78000,"queue":1,"seconds":5}"#
            ),
            Some(Event::AppProgress {
                appid: 78000,
                bytes_downloaded: 81_160_832,
                bytes_to_download: 81_160_832,
                bytes_staged: 97_849_494,
                bytes_to_stage: 97_849_494,
            })
        );
        assert_eq!(
            parse(
                r#"{"event":"sync_finished","appid":584400,"kind":"launch","result":"offline","state":0,"seconds":0}"#
            ),
            Some(Event::SyncFinished {
                appid: 584400,
                kind: "launch".into(),
                result: "offline".into()
            })
        );
        assert_eq!(
            parse(
                r#"{"event":"sync_conflict","appid":2379780,"found":true,"local_time":1763300000,"remote_time":1763390000}"#
            ),
            Some(Event::SyncConflict {
                appid: 2379780,
                local_time: 1_763_300_000,
                remote_time: 1_763_390_000
            })
        );
        assert_eq!(
            parse(
                r#"{"event":"logon_state","state":4,"logged_on":true,"connected":true,"offline":false}"#
            ),
            Some(Event::LogonState {
                logged_on: true,
                offline: false
            })
        );
        assert_eq!(
            parse(r#"{"event":"qr_code","challenge_url":"https://s.team/q/1/2"}"#),
            Some(Event::QrCode {
                challenge_url: "https://s.team/q/1/2".into()
            })
        );
        assert_eq!(
            parse(r#"{"event":"app","appid":70,"name":"Half-Life","source":"family"}"#),
            Some(Event::App {
                appid: 70,
                name: "Half-Life".into(),
                source: AppSource::Family
            })
        );
        assert_eq!(
            parse(r#"{"event":"signed_out","reason":"no_saved_login"}"#),
            Some(Event::SignedOut {
                reason: Some("no_saved_login".into())
            })
        );
        assert_eq!(
            parse(r#"{"event":"signed_out"}"#),
            Some(Event::SignedOut { reason: None })
        );
        assert_eq!(
            parse(
                r#"{"event":"launch_refused","reason":"install_failed","message":"Steam could not install or update the game."}"#
            ),
            Some(Event::LaunchRefused {
                reason: "install_failed".into(),
                message: "Steam could not install or update the game.".into()
            })
        );
        assert_eq!(
            parse(r#"{"event":"login_saved","confirmed":true}"#),
            Some(Event::LoginSaved { confirmed: true })
        );
        assert_eq!(
            parse(r#"{"event":"command","command":"shutdown","deferred":"login_save"}"#),
            Some(Event::Command {
                command: "shutdown".into(),
                deferred: Some("login_save".into())
            })
        );
        assert_eq!(
            parse(r#"{"event":"command","command":"signin-qr"}"#),
            Some(Event::Command {
                command: "signin-qr".into(),
                deferred: None
            })
        );
        assert_eq!(
            parse(r#"{"event":"engine_map_missing"}"#),
            Some(Event::EngineMapMissing {})
        );
        assert_eq!(
            parse(r#"{"event":"engine_mismatch","expected":"aa","found":"bb"}"#),
            Some(Event::EngineMismatch {})
        );
        assert_eq!(
            parse(r#"{"event":"engine_refused","eresult":120}"#),
            Some(Event::EngineRefused { eresult: 120 })
        );
        assert_eq!(
            parse(r#"{"event":"callback","id":101}"#),
            Some(Event::Other)
        );
        assert_eq!(
            parse(
                r#"{"event":"save_export","appid":70,"archive":"/run/korri-steam-host-x/channel/saves/70-exit-2.tar"}"#
            ),
            Some(Event::SaveExport {
                appid: 70,
                archive: "/run/korri-steam-host-x/channel/saves/70-exit-2.tar".into()
            })
        );
        assert_eq!(
            parse(r#"{"event":"save_export_empty","appid":70,"phase":"exit"}"#),
            Some(Event::SaveExportEmpty { appid: 70 })
        );
        for other in [
            "session_over",
            "apps_skipped",
            "mode",
            "login_token_set",
            "qr_failure",
            "save_export_failed",
        ] {
            assert_eq!(
                parse(&format!(r#"{{"event":"{other}"}}"#)),
                Some(Event::Other),
                "{other}"
            );
        }
        assert_eq!(parse("korri-steam-host: error"), None);
        assert_eq!(
            parse(r#"{"event":"app","appid":-1,"name":"x","source":"owned"}"#),
            None
        );
        assert_eq!(
            parse(r#"{"event":"app","appid":1,"name":"x","source":"stolen"}"#),
            None
        );
    }

    #[test]
    fn sync_and_app_results_map_to_what_korrid_does_next() {
        for ready in [
            "downloaded",
            "synchronized",
            "local_changes",
            "cloud_disabled",
            "no_sync_state",
        ] {
            assert_eq!(sync_outcome(ready), SyncOutcome::Ready);
        }
        assert_eq!(sync_outcome("offline"), SyncOutcome::Offline);
        assert_eq!(sync_outcome("conflict"), SyncOutcome::Conflict);
        assert_eq!(sync_outcome("timeout"), SyncOutcome::Incomplete);
        assert!(app_job_succeeded("installed") && app_job_succeeded("uninstalled"));
        assert!(!app_job_succeeded("install_failed") && !app_job_succeeded("timeout"));
    }
}
