//! Port of `clients/portal/src/surface/launch-settings.ts`: Device settings'
//! launch groups, and one game's editor, opened from a game action and
//! closed by its close action. Every write is one typed
//! `app.launch-settings.set` naming a place, a path and the revision of the
//! snapshot it was made from. Each read and write has one generation, so a
//! late reply changes nothing.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use korri_surface_treaty::{
    SurfaceAction, SurfaceLaunchProfile, SurfaceLaunchSettings, SurfaceLaunchSettingsContent,
    SurfaceLaunchSettingsProblem, SurfaceSettingGroup, SurfaceSettingInteraction,
    SurfaceSettingItem, SurfaceSettingsStatus,
};
use serde_json::Value;

use super::{
    korrid::{ask, Korrid},
    launch_model::{
        active_profile_of, device_launch_from, launch_editor_from, launch_id, profile_from_pick,
        revision_of, DeviceLaunch, LaunchEditor, LaunchWrite, PendingSetting,
    },
    Notify,
};
use crate::{
    launch_settings::{
        LaunchProfileRemoveRequest, LaunchProfileSaveRequest, LaunchProfileSelectRequest,
        LaunchSetRequest, LaunchSettingsSnapshotOutcome, LaunchSettingsSnapshotRequest,
        LaunchSettingsWriteOutcome,
    },
    RpcFailure,
};

pub const LAUNCH_SETTINGS_ACTION_ID: &str = "launch-settings";
const CONFLICT: &str =
    "Launch settings changed elsewhere. Reload to see them before you change them.";
const INVALID: &str = "That value isn't allowed here.";

fn close_action() -> SurfaceAction {
    SurfaceAction::new(launch_id::CANCEL, "Close", true)
}
fn reload_action() -> SurfaceAction {
    SurfaceAction::new(launch_id::RELOAD, "Reload", true)
}
fn retry_action() -> SurfaceAction {
    SurfaceAction::new(launch_id::RETRY, "Retry", true)
}

fn device_error_group(message: &str) -> SurfaceSettingGroup {
    SurfaceSettingGroup {
        title: "Launch settings".into(),
        items: vec![SurfaceSettingItem {
            description: Some(message.into()),
            interaction: Some(SurfaceSettingInteraction::Action {
                action_id: launch_id::DEVICE_RETRY.into(),
                destructive: None,
                confirmation: None,
            }),
            ..SurfaceSettingItem::new(launch_id::DEVICE_RETRY, "Launch settings could not be read")
        }],
    }
}

/// One write the editor or Device settings sends, from the snapshot it was
/// made from.
#[derive(Clone)]
enum Write {
    Set {
        target: LaunchWrite,
        value: Option<Value>,
        clear: bool,
    },
    Select {
        game_id: String,
        profile_id: Option<String>,
    },
    Save {
        game_id: String,
        profile_id: Option<String>,
        title: String,
    },
    Remove {
        game_id: String,
        profile_id: String,
    },
}

#[derive(Clone)]
enum Again {
    Read,
    Write(String, Box<Write>),
}

pub type OpenRunners = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct Inner {
    editor: Option<SurfaceLaunchSettings>,
    device_groups: Option<Vec<SurfaceSettingGroup>>,
    status: Option<SurfaceSettingsStatus>,
    active_profiles: BTreeMap<String, Option<String>>,
    generation: u64,
    device_generation: u64,
    busy: bool,
    game_id: Option<String>,
    game_title: String,
    open_runners: Option<OpenRunners>,
    snapshot: Option<Value>,
    editor_rows: Option<LaunchEditor>,
    device_snapshot: Option<Value>,
    device: Option<DeviceLaunch>,
    device_pending: HashMap<String, Vec<PendingSetting>>,
    pending: HashMap<String, Vec<PendingSetting>>,
    learning: std::collections::HashSet<String>,
    again: Option<Again>,
}

pub struct LaunchSettings {
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
}

fn still_pending(rows: &super::launch_model::Rows) -> HashMap<String, Vec<PendingSetting>> {
    let mut still: HashMap<String, Vec<PendingSetting>> = HashMap::new();
    for (place_id, setting) in rows.pending.values() {
        still
            .entry(place_id.clone())
            .or_default()
            .push(setting.clone());
    }
    still
}

impl Inner {
    fn editor_tag(&self) -> &'static str {
        match &self.editor {
            None | Some(SurfaceLaunchSettings::Closed) => "Closed",
            Some(SurfaceLaunchSettings::Loading(_)) => "Loading",
            Some(SurfaceLaunchSettings::Ready(_)) => "Ready",
            Some(SurfaceLaunchSettings::Conflict(_)) => "Conflict",
            Some(SurfaceLaunchSettings::Error(_)) => "Error",
        }
    }

    fn build(&mut self) {
        self.editor_rows = self
            .snapshot
            .as_ref()
            .map(|snapshot| launch_editor_from(snapshot, self.game_id.as_deref(), &self.pending));
        self.pending = self
            .editor_rows
            .as_ref()
            .map(|editor| still_pending(&editor.rows))
            .unwrap_or_default();
    }

    fn build_device(&mut self) {
        let Some(snapshot) = &self.device_snapshot else {
            return;
        };
        let device = device_launch_from(snapshot, &self.device_pending);
        self.device_pending = still_pending(&device.rows);
        self.device_groups = Some(device.groups.clone());
        self.device = Some(device);
    }

    fn content(&self, actions: Vec<SurfaceAction>) -> SurfaceLaunchSettingsContent {
        let profiles = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.get("profiles"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        SurfaceLaunchSettingsContent {
            game_title: Some(self.game_title.clone()),
            places: self
                .editor_rows
                .as_ref()
                .map(|editor| editor.places.clone())
                .unwrap_or_default(),
            profiles: profiles
                .iter()
                .map(|profile| SurfaceLaunchProfile {
                    id: profile
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    title: profile
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                })
                .collect(),
            selected_profile_id: self
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.get("selectedProfile"))
                .and_then(Value::as_str)
                .map(Into::into),
            notes: self
                .editor_rows
                .as_ref()
                .map(|editor| editor.notes.clone())
                .unwrap_or_default(),
            actions,
        }
    }

    /// Choose a runner, when the places wait on one and the page offers it.
    fn runner_action(&self) -> Vec<SurfaceAction> {
        let needs = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.get("routeLayers"))
            .and_then(|layers| layers.get("_tag"))
            .and_then(Value::as_str)
            == Some("NeedsRunnerChoice");
        if needs && self.open_runners.is_some() {
            vec![SurfaceAction::new(
                launch_id::RUNNER,
                "Choose a runner",
                true,
            )]
        } else {
            Vec::new()
        }
    }

    fn ready(&mut self) {
        self.build();
        let mut actions = self.runner_action();
        actions.extend([reload_action(), close_action()]);
        self.editor = Some(SurfaceLaunchSettings::Ready(self.content(actions)));
    }

    fn failed(&mut self, failure: &RpcFailure, again: Again) {
        self.again = Some(again);
        self.editor = Some(if failure.code == "SettingsConflict" {
            SurfaceLaunchSettings::Conflict(SurfaceLaunchSettingsProblem {
                message: CONFLICT.into(),
                content: self.content(vec![reload_action(), close_action()]),
            })
        } else {
            SurfaceLaunchSettings::Error(SurfaceLaunchSettingsProblem {
                message: failure.message.clone(),
                content: self.content(vec![retry_action(), close_action()]),
            })
        });
    }

    fn remember(&mut self, id: &str, snapshot: &Value) {
        self.active_profiles
            .insert(id.into(), active_profile_of(snapshot));
    }
}

fn revision(snapshot: &Value, layer: &Value) -> String {
    snapshot
        .get("revisions")
        .and_then(|revisions| revisions.get(revision_of(layer)))
        .and_then(Value::as_str)
        .unwrap_or("")
        .into()
}

impl LaunchSettings {
    pub fn new(korrid: Korrid, notify: Notify) -> Arc<Self> {
        Arc::new(Self {
            korrid,
            inner: Mutex::new(Inner::default()),
            notify,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("launch settings poisoned")
    }

    fn changed(&self) {
        (self.notify)();
    }

    pub fn editor(&self) -> SurfaceLaunchSettings {
        self.lock()
            .editor
            .clone()
            .unwrap_or(SurfaceLaunchSettings::Closed)
    }
    pub fn device_groups(&self) -> Option<Vec<SurfaceSettingGroup>> {
        self.lock().device_groups.clone()
    }
    pub fn status(&self) -> Option<SurfaceSettingsStatus> {
        self.lock().status.clone()
    }
    pub fn active_profiles(&self) -> BTreeMap<String, Option<String>> {
        self.lock().active_profiles.clone()
    }
    pub fn is_open(&self) -> bool {
        self.lock().editor_tag() != "Closed"
    }

    async fn snapshot(&self, game_id: Option<String>) -> Result<Value, RpcFailure> {
        match ask!(
            self.korrid,
            Route,
            LaunchSettingsSnapshot,
            LaunchSettingsSnapshotRequest { game_id },
            LaunchSettingsSnapshotOutcome
        ) {
            LaunchSettingsSnapshotOutcome::Ok(snapshot) => {
                Ok(serde_json::to_value(*snapshot).unwrap_or(Value::Null))
            }
            LaunchSettingsSnapshotOutcome::Err(failure) => Err(failure),
        }
    }

    pub async fn refresh_device(&self) {
        let operation = {
            let mut inner = self.lock();
            inner.device_generation += 1;
            inner.device_generation
        };
        let result = self.snapshot(None).await;
        {
            let mut inner = self.lock();
            if operation != inner.device_generation {
                return;
            }
            match result {
                Err(failure) => {
                    // The group stays, saying what went wrong, with Retry.
                    inner.device_snapshot = None;
                    inner.device = None;
                    inner.device_groups = Some(vec![device_error_group(&failure.message)]);
                }
                Ok(snapshot) => {
                    inner.device_snapshot = Some(snapshot);
                    inner.build_device();
                }
            }
        }
        self.changed();
    }

    fn spawn_refresh_device(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move { this.refresh_device().await });
    }

    /// The synchronous half of a read: Loading shows at once, as the
    /// portal's read published it before its first await.
    fn begin_read(&self, keep: bool) -> Option<(String, u64)> {
        let started = {
            let mut inner = self.lock();
            let id = inner.game_id.clone()?;
            inner.generation += 1;
            if !keep {
                inner.snapshot = None;
                inner.editor_rows = None;
                inner.editor = Some(SurfaceLaunchSettings::Loading(
                    inner.content(vec![close_action()]),
                ));
            }
            (id, inner.generation)
        };
        self.changed();
        Some(started)
    }

    async fn read(self: &Arc<Self>, keep: bool) {
        if let Some((id, operation)) = self.begin_read(keep) {
            self.finish_read(id, operation).await;
        }
    }

    async fn finish_read(&self, id: String, operation: u64) {
        let result = self.snapshot(Some(id.clone())).await;
        {
            let mut inner = self.lock();
            if operation != inner.generation {
                return;
            }
            match result {
                Err(failure) => inner.failed(&failure, Again::Read),
                Ok(snapshot) => {
                    inner.remember(&id, &snapshot);
                    inner.snapshot = Some(snapshot);
                    inner.again = None;
                    inner.ready();
                }
            }
        }
        self.changed();
    }

    fn spawn_read(self: &Arc<Self>, keep: bool) {
        if let Some((id, operation)) = self.begin_read(keep) {
            let this = Arc::clone(self);
            tokio::spawn(async move { this.finish_read(id, operation).await });
        }
    }

    async fn send(&self, send: Write, snapshot: &Value) -> LaunchSettingsWriteOutcome {
        match send {
            Write::Set {
                target,
                value,
                clear,
            } => {
                let (Ok(place), Ok(path)) = (
                    serde_json::from_value(target.place.clone()),
                    serde_json::from_value(target.path.clone()),
                ) else {
                    return LaunchSettingsWriteOutcome::Err(RpcFailure {
                        code: "SettingsInvalid".into(),
                        message: INVALID.into(),
                    });
                };
                let layer = target.place.get("layer").cloned().unwrap_or(Value::Null);
                ask!(
                    self.korrid,
                    Route,
                    LaunchSettingsSet,
                    LaunchSetRequest {
                        place,
                        path,
                        value,
                        clear,
                        expected_revision: revision(snapshot, &layer)
                    },
                    LaunchSettingsWriteOutcome
                )
            }
            Write::Select {
                game_id,
                profile_id,
            } => ask!(
                self.korrid,
                Route,
                LaunchProfileSelect,
                LaunchProfileSelectRequest {
                    game_id,
                    profile_id,
                    expected_revision: revision(snapshot, &serde_json::json!({ "_tag": "Game" }))
                },
                LaunchSettingsWriteOutcome
            ),
            Write::Save {
                game_id,
                profile_id,
                title,
            } => ask!(
                self.korrid,
                Route,
                LaunchProfileSave,
                LaunchProfileSaveRequest {
                    game_id,
                    profile_id,
                    title,
                    expected_revision: revision(snapshot, &serde_json::json!({ "_tag": "Game" }))
                },
                LaunchSettingsWriteOutcome
            ),
            Write::Remove {
                game_id,
                profile_id,
            } => ask!(
                self.korrid,
                Route,
                LaunchProfileRemove,
                LaunchProfileRemoveRequest {
                    game_id,
                    profile_id,
                    expected_revision: revision(snapshot, &serde_json::json!({ "_tag": "Game" }))
                },
                LaunchSettingsWriteOutcome
            ),
        }
    }

    /// One write from the editor. Saving shows on the row; the snapshot is
    /// read again after.
    fn write(self: &Arc<Self>, setting_id: String, send: Write) {
        let (snapshot, operation) = {
            let mut inner = self.lock();
            let Some(snapshot) = inner.snapshot.clone() else {
                return;
            };
            if inner.busy || inner.editor_tag() != "Ready" {
                return;
            }
            inner.generation += 1;
            inner.busy = true;
            inner.status = Some(SurfaceSettingsStatus::Saving {
                setting_id: setting_id.clone(),
            });
            (snapshot, inner.generation)
        };
        self.changed();
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let result = this.send(send.clone(), &snapshot).await;
            let open = {
                let mut inner = this.lock();
                inner.busy = false;
                inner.status = None;
                operation == inner.generation
            };
            if !open {
                this.changed();
                return;
            }
            if let LaunchSettingsWriteOutcome::Err(failure) = result {
                this.lock()
                    .failed(&failure, Again::Write(setting_id, Box::new(send)));
                this.changed();
                return;
            }
            this.changed();
            this.spawn_refresh_device();
            this.read(true).await;
        });
    }

    fn set_from(&self, setting_id: &str, target: &LaunchWrite, text: &str) -> Option<Write> {
        match target.encoder.encode(text) {
            None => {
                self.lock().status = Some(SurfaceSettingsStatus::Problem {
                    setting_id: setting_id.into(),
                    message: INVALID.into(),
                });
                self.changed();
                None
            }
            Some(encoded) => Some(Write::Set {
                target: target.clone(),
                value: encoded.value,
                clear: encoded.clear,
            }),
        }
    }

    fn write_device(self: &Arc<Self>, setting_id: String, send: Option<Write>) {
        let snapshot = {
            let mut inner = self.lock();
            if inner.busy {
                return;
            }
            let Some(snapshot) = inner.device_snapshot.clone() else {
                inner.status = Some(SurfaceSettingsStatus::Problem {
                    setting_id,
                    message: "Launch settings are not available.".into(),
                });
                drop(inner);
                self.changed();
                return;
            };
            let Some(_) = &send else { return };
            inner.busy = true;
            inner.status = Some(SurfaceSettingsStatus::Saving {
                setting_id: setting_id.clone(),
            });
            snapshot
        };
        self.changed();
        let Some(send) = send else { return };
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let result = this.send(send, &snapshot).await;
            {
                let mut inner = this.lock();
                inner.busy = false;
                inner.status = match result {
                    LaunchSettingsWriteOutcome::Err(failure) => {
                        Some(SurfaceSettingsStatus::Problem {
                            setting_id,
                            message: if failure.code == "SettingsConflict" {
                                "Launch settings changed elsewhere. They are shown again; choose once more.".into()
                            } else {
                                failure.message
                            },
                        })
                    }
                    LaunchSettingsWriteOutcome::Ok(_) => None,
                };
            }
            this.changed();
            this.refresh_device().await;
        });
    }

    fn withdraw_device(&self, setting_id: &str) -> bool {
        let mut inner = self.lock();
        let Some((place_id, setting)) = inner
            .device
            .as_ref()
            .and_then(|device| device.rows.pending.get(setting_id))
            .cloned()
        else {
            return false;
        };
        inner
            .device_pending
            .entry(place_id)
            .or_default()
            .retain(|candidate| *candidate != setting);
        inner.build_device();
        drop(inner);
        self.changed();
        true
    }

    fn change_device(self: &Arc<Self>, setting_id: &str, text: &str) {
        let adding = self
            .lock()
            .device
            .as_ref()
            .and_then(|device| device.rows.adds.get(setting_id))
            .cloned();
        if let Some(adding) = adding {
            if text.is_empty() {
                return;
            }
            let setting = adding.kind.pending_of(text);
            let mut inner = self.lock();
            let settings = inner.device_pending.entry(adding.place_id).or_default();
            if settings.contains(&setting) {
                return;
            }
            settings.push(setting);
            inner.build_device();
            drop(inner);
            self.changed();
            return;
        }
        let target = self
            .lock()
            .device
            .as_ref()
            .and_then(|device| device.rows.writes.get(setting_id))
            .cloned();
        let Some(target) = target else {
            self.lock().status = Some(SurfaceSettingsStatus::Problem {
                setting_id: setting_id.into(),
                message: "Launch settings are not available.".into(),
            });
            self.changed();
            return;
        };
        if let Some(encoded) = target.encoder.encode(text) {
            if encoded.value.is_none() && !encoded.clear && self.withdraw_device(setting_id) {
                return;
            }
        }
        let send = self.set_from(setting_id, &target, text);
        self.write_device(setting_id.into(), send);
    }

    fn act_device(self: &Arc<Self>, action_id: &str) -> bool {
        let (cleared, reset) = {
            let inner = self.lock();
            let rows = inner.device.as_ref().map(|device| &device.rows);
            (
                rows.and_then(|rows| rows.clears.get(action_id)).cloned(),
                rows.and_then(|rows| rows.defaults.get(action_id)).cloned(),
            )
        };
        let Some(setting_id) = cleared.clone().or(reset.clone()) else {
            return false;
        };
        if cleared.is_some() && self.withdraw_device(&setting_id) {
            return true;
        }
        let target = self
            .lock()
            .device
            .as_ref()
            .and_then(|device| device.rows.writes.get(&setting_id))
            .cloned();
        if let Some(target) = target {
            self.write_device(
                action_id.into(),
                Some(Write::Set {
                    target,
                    value: None,
                    clear: reset.is_some(),
                }),
            );
        }
        true
    }

    fn add(&self, place_id: String, setting: PendingSetting) {
        let mut inner = self.lock();
        if inner.editor_tag() != "Ready" {
            return;
        }
        let settings = inner.pending.entry(place_id).or_default();
        if settings.contains(&setting) {
            return;
        }
        settings.push(setting);
        inner.ready();
        drop(inner);
        self.changed();
    }

    fn withdraw(&self, setting_id: &str) -> bool {
        let mut inner = self.lock();
        let Some((place_id, setting)) = inner
            .editor_rows
            .as_ref()
            .and_then(|editor| editor.rows.pending.get(setting_id))
            .cloned()
        else {
            return false;
        };
        inner
            .pending
            .entry(place_id)
            .or_default()
            .retain(|candidate| *candidate != setting);
        inner.ready();
        drop(inner);
        self.changed();
        true
    }

    /// Changes to a launch row. False: not a launch row.
    pub fn change(self: &Arc<Self>, setting_id: &str, value: &str) -> bool {
        if setting_id.starts_with("launch-device:") {
            self.change_device(setting_id, value);
            return true;
        }
        let (game, target, adding, snapshot) = {
            let inner = self.lock();
            let rows = inner.editor_rows.as_ref().map(|editor| &editor.rows);
            (
                inner.game_id.clone(),
                rows.and_then(|rows| rows.writes.get(setting_id)).cloned(),
                rows.and_then(|rows| rows.adds.get(setting_id)).cloned(),
                inner.snapshot.clone(),
            )
        };
        if let Some(target) = target {
            if let Some(encoded) = target.encoder.encode(value) {
                if encoded.value.is_none() && !encoded.clear && self.withdraw(setting_id) {
                    return true;
                }
            }
            if let Some(send) = self.set_from(setting_id, &target, value) {
                self.write(setting_id.into(), send);
            }
            return true;
        }
        if let Some(adding) = adding {
            if !value.is_empty() {
                self.add(adding.place_id, adding.kind.pending_of(value));
            }
            return true;
        }
        if setting_id == launch_id::PICK {
            let profile_id = profile_from_pick(value);
            // Only a profile this read published.
            let known = profile_id.as_ref().is_none_or(|id| {
                snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.get("profiles"))
                    .and_then(Value::as_array)
                    .is_some_and(|profiles| {
                        profiles
                            .iter()
                            .any(|profile| profile.get("id").and_then(Value::as_str) == Some(id))
                    })
            });
            if let (Some(game_id), true) = (game, known) {
                self.write(
                    setting_id.into(),
                    Write::Select {
                        game_id,
                        profile_id,
                    },
                );
            }
            return true;
        }
        let renaming = setting_id
            .strip_prefix("launch:profile:rename:")
            .map(str::to_owned);
        if setting_id == launch_id::CREATE || renaming.is_some() {
            let title = value.trim().to_owned();
            if title.is_empty() {
                // A profile needs a name; korrid would refuse it too.
                self.lock().status = Some(SurfaceSettingsStatus::Problem {
                    setting_id: setting_id.into(),
                    message: "A profile needs a name.".into(),
                });
                self.changed();
                return true;
            }
            if let Some(game_id) = game {
                self.write(
                    setting_id.into(),
                    Write::Save {
                        game_id,
                        profile_id: renaming,
                        title,
                    },
                );
            }
            return true;
        }
        setting_id.starts_with("launch:")
    }

    /// Editor actions and profile deletion. False: not a launch action.
    pub fn act(self: &Arc<Self>, action_id: &str) -> bool {
        if action_id == launch_id::DEVICE_RETRY {
            self.spawn_refresh_device();
            return true;
        }
        if action_id.starts_with("launch-device:") {
            return self.act_device(action_id);
        }
        if action_id == launch_id::CANCEL {
            self.cancel();
            return true;
        }
        if action_id == launch_id::RUNNER {
            let open = {
                let inner = self.lock();
                (inner.editor_tag() != "Closed" && !inner.runner_action().is_empty())
                    .then(|| inner.open_runners.clone())
                    .flatten()
            };
            if let Some(open) = open {
                open();
            }
            return true;
        }
        if action_id == launch_id::RELOAD {
            let tag = self.lock().editor_tag();
            if tag != "Closed" {
                self.spawn_read(tag == "Ready");
            }
            return true;
        }
        if action_id == launch_id::RETRY {
            let again = {
                let mut inner = self.lock();
                if inner.editor_tag() != "Error" {
                    return true;
                }
                let again = inner.again.take().unwrap_or(Again::Read);
                // Retry shows the editor again first, so a repeated write has a row to save on.
                if inner.snapshot.is_some() {
                    inner.ready();
                }
                again
            };
            self.changed();
            match again {
                Again::Read => self.spawn_read(false),
                Again::Write(setting_id, send) => self.write(setting_id, *send),
            }
            return true;
        }
        let (cleared, reset, game_id, snapshot) = {
            let inner = self.lock();
            let rows = inner.editor_rows.as_ref().map(|editor| &editor.rows);
            (
                rows.and_then(|rows| rows.clears.get(action_id)).cloned(),
                rows.and_then(|rows| rows.defaults.get(action_id)).cloned(),
                inner.game_id.clone(),
                inner.snapshot.clone(),
            )
        };
        let target_of = |this: &Self, id: &str| {
            this.lock()
                .editor_rows
                .as_ref()
                .and_then(|editor| editor.rows.writes.get(id))
                .cloned()
        };
        // Inherit on a string setting: a pending one goes, a written one is taken away.
        if let Some(cleared) = cleared {
            if self.withdraw(&cleared) {
                return true;
            }
            if let Some(target) = target_of(self, &cleared) {
                self.write(
                    action_id.into(),
                    Write::Set {
                        target,
                        value: None,
                        clear: false,
                    },
                );
            }
            return true;
        }
        // Default on a number or a string setting: a Cleared setting here.
        if let Some(reset) = reset {
            if let Some(target) = target_of(self, &reset) {
                self.write(
                    action_id.into(),
                    Write::Set {
                        target,
                        value: None,
                        clear: true,
                    },
                );
            }
            return true;
        }
        let Some(profile_id) = action_id.strip_prefix("launch:profile:remove:") else {
            return false;
        };
        let known = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.get("profiles"))
            .and_then(Value::as_array)
            .is_some_and(|profiles| {
                profiles
                    .iter()
                    .any(|profile| profile.get("id").and_then(Value::as_str) == Some(profile_id))
            });
        if let (Some(game_id), true) = (game_id, known) {
            self.write(
                action_id.into(),
                Write::Remove {
                    game_id,
                    profile_id: profile_id.into(),
                },
            );
        }
        true
    }

    pub fn cancel(&self) {
        {
            let mut inner = self.lock();
            inner.generation += 1;
            inner.game_id = None;
            inner.open_runners = None;
            inner.snapshot = None;
            inner.editor_rows = None;
            inner.pending.clear();
            inner.again = None;
            inner.editor = Some(SurfaceLaunchSettings::Closed);
            if !inner.busy {
                inner.status = None;
            }
        }
        self.changed();
    }

    /// Read one game's profile pick for its page, once.
    pub fn learn(self: &Arc<Self>, id: &str) {
        {
            let mut inner = self.lock();
            if inner.learning.contains(id) || inner.active_profiles.contains_key(id) {
                return;
            }
            inner.learning.insert(id.into());
        }
        let this = Arc::clone(self);
        let id = id.to_owned();
        tokio::spawn(async move {
            let result = this.snapshot(Some(id.clone())).await;
            let mut inner = this.lock();
            inner.learning.remove(&id);
            if let Ok(snapshot) = result {
                inner.remember(&id, &snapshot);
                drop(inner);
                this.changed();
            }
        });
    }

    /// Open the editor for one game, by korrid's id.
    pub fn open(self: &Arc<Self>, id: String, title: String, choose_runners: Option<OpenRunners>) {
        {
            let mut inner = self.lock();
            inner.game_id = Some(id);
            inner.game_title = title;
            inner.open_runners = choose_runners;
            inner.pending.clear();
        }
        self.spawn_read(false);
    }

    /// Dismiss a launch row's problem. False: there is none.
    pub fn dismiss_problem(&self) -> bool {
        let dismissed = {
            let mut inner = self.lock();
            if !matches!(inner.status, Some(SurfaceSettingsStatus::Problem { .. })) {
                return false;
            }
            inner.status = None;
            true
        };
        self.changed();
        dismissed
    }
}
