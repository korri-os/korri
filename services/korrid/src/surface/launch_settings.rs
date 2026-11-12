//! Launch settings for the surface: Device settings' launch groups, and one
//! game's editor, opened from a game action and closed by its close action.
//! The editor is one list (ADR 0066): a changed row waits as a change, and the
//! changes are written when the editor closes, to the place the person chose.
//! A streamed game's list is read from and written to the device that runs
//! it. Every write is one typed `app.launch-settings.set` naming a place, a
//! path and the revision of the snapshot it was made from. Each read and
//! write has one generation, so a late reply changes nothing.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use korri_surface_treaty::{
    SurfaceAction, SurfaceLaunchProfile, SurfaceLaunchSettings, SurfaceLaunchSettingsContent,
    SurfaceLaunchSettingsProblem, SurfaceSettingGroup, SurfaceSettingInteraction,
    SurfaceSettingItem, SurfaceSettingsStatus,
};
use serde_json::{json, Value};

use super::{
    korrid::{ask, Korrid},
    launch_model::{
        active_profile_of, device_launch_from, launch_id,
        list::{launch_list, LaunchList, ListInput, RowTarget, KEEP_LAUNCH},
        profile_from_pick, revision_of, DeviceLaunch, LaunchWrite, PendingSetting,
    },
    one_launch::{game_key, Change, OneLaunch},
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
    /// Close again: write the changes that are left.
    Keep,
}

pub type OpenRunners = Arc<dyn Fn() + Send + Sync>;

/// The game the editor is open for, and the device that runs it.
#[derive(Clone)]
struct Subject {
    id: String,
    title: String,
    /// The label of the device that runs a streamed game. None: this device.
    host: Option<String>,
}

impl Subject {
    fn key(&self) -> String {
        game_key(&self.id, self.host.as_deref())
    }
}

/// One close that writes: the changes, their place and the snapshot they
/// were made from.
struct Keeping {
    game: Subject,
    layer: Value,
    changes: Vec<Change>,
    snapshot: Value,
    operation: u64,
}

/// What a close has to do with the changes that wait.
enum Keep {
    /// Nothing waits to be written.
    Nothing,
    Write(Keeping),
    /// Changes wait, and the editor cannot write them now: it is not Ready,
    /// or another write is on its way.
    Cannot,
}

/// How a close ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closing {
    /// No change waits to be written. The editor is closed.
    Kept,
    /// A change could not be written. It stays marked, and an open editor
    /// stays open and says why.
    NotKept,
}

/// The holder did not answer: no reply, a reply that was not korrid's, or no
/// such peer right now. A refusal the holder itself gave has its own code.
fn holder_did_not_answer(failure: &RpcFailure) -> bool {
    failure.code.starts_with("Upstream")
}

#[derive(Default)]
struct Inner {
    editor: Option<SurfaceLaunchSettings>,
    device_groups: Option<Vec<SurfaceSettingGroup>>,
    status: Option<SurfaceSettingsStatus>,
    active_profiles: BTreeMap<String, Option<String>>,
    generation: u64,
    device_generation: u64,
    busy: bool,
    game: Option<Subject>,
    open_runners: Option<OpenRunners>,
    snapshot: Option<Value>,
    /// The device that runs the game did not answer the last read or write.
    unreachable: bool,
    /// The last snapshot each game's device gave, to show its rows while it
    /// cannot be reached. They cannot change then.
    known: HashMap<String, Value>,
    list: Option<LaunchList>,
    drafts: OneLaunch,
    device_snapshot: Option<Value>,
    device: Option<DeviceLaunch>,
    device_pending: HashMap<String, Vec<PendingSetting>>,
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

    fn list_now(&self, game: &Subject) -> LaunchList {
        let key = game.key();
        launch_list(&ListInput {
            // A holder that cannot be reached shows what it last said.
            snapshot: self
                .snapshot
                .as_ref()
                .or_else(|| self.known.get(&key).filter(|_| self.unreachable)),
            game_id: &game.id,
            holder: game.host.as_deref(),
            reachable: !self.unreachable,
            viewer: self.device_snapshot.as_ref(),
            changes: self.drafts.changes(&key),
            keep: self.drafts.keep(&key),
            can_choose_runner: self.open_runners.is_some(),
        })
    }

    fn build(&mut self) {
        let Some(game) = self.game.clone() else {
            self.list = None;
            return;
        };
        let mut list = self.list_now(&game);
        if self.snapshot.is_some() && !self.unreachable {
            // A change whose row is gone, or whose value the device now has,
            // could not be seen or set back.
            let key = game.key();
            let before = self.drafts.changes(&key).len();
            self.drafts.retain(&key, |change| {
                list.rows
                    .get(&change.row)
                    .is_some_and(|row| row.stored != change.value)
            });
            if self.drafts.changes(&key).len() != before {
                list = self.list_now(&game);
            }
        }
        self.list = Some(list);
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
        let list = self.list.as_ref();
        SurfaceLaunchSettingsContent {
            game_title: self.game.as_ref().map(|game| game.title.clone()),
            runs_on: self.game.as_ref().map(|game| match &game.host {
                Some(host) => format!("Runs on {host}"),
                None => "Runs here".into(),
            }),
            groups: list.map(|list| list.groups.clone()).unwrap_or_default(),
            changes: list.map(|list| list.changes.clone()).unwrap_or_default(),
            keep: list.and_then(|list| list.keep.clone()),
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
            notes: list.map(|list| list.notes.clone()).unwrap_or_default(),
            actions,
        }
    }

    /// Choose a runner, when the list waits on one and the page offers it.
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
        self.unreachable = false;
        self.build();
        let mut actions = self.runner_action();
        actions.extend([reload_action(), close_action()]);
        self.editor = Some(SurfaceLaunchSettings::Ready(self.content(actions)));
    }

    fn failed(&mut self, failure: &RpcFailure, again: Again) {
        self.again = Some(again);
        if failure.code == "SettingsConflict" {
            self.build();
            self.editor = Some(SurfaceLaunchSettings::Conflict(
                SurfaceLaunchSettingsProblem {
                    message: CONFLICT.into(),
                    content: self.content(vec![reload_action(), close_action()]),
                },
            ));
            return;
        }
        let holder = self
            .game
            .as_ref()
            .and_then(|game| game.host.clone())
            .filter(|_| holder_did_not_answer(failure));
        self.unreachable = holder.is_some();
        self.build();
        self.editor = Some(SurfaceLaunchSettings::Error(SurfaceLaunchSettingsProblem {
            message: match holder {
                Some(holder) => {
                    format!("{holder} cannot be reached. Its settings cannot change now.")
                }
                None => failure.message.clone(),
            },
            content: self.content(vec![retry_action(), close_action()]),
        }));
    }

    /// This device's own rows changed under an open editor: draw it again
    /// in the state it is in.
    fn redraw(&mut self) {
        match self.editor.take() {
            Some(SurfaceLaunchSettings::Ready(_)) => self.ready(),
            Some(SurfaceLaunchSettings::Error(problem)) => {
                self.build();
                self.editor = Some(SurfaceLaunchSettings::Error(SurfaceLaunchSettingsProblem {
                    content: self.content(problem.content.actions),
                    ..problem
                }));
            }
            other => self.editor = other,
        }
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

    /// One device's launch settings: this device's, or the holder's that
    /// `host` names.
    async fn snapshot(
        &self,
        game_id: Option<String>,
        host: Option<String>,
    ) -> Result<Value, RpcFailure> {
        match ask!(
            self.korrid,
            Route,
            LaunchSettingsSnapshot,
            LaunchSettingsSnapshotRequest {
                game_id,
                holder: host
            },
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
        let result = self.snapshot(None, None).await;
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
            // A streamed game's list shows this device's stream section.
            if inner.game.as_ref().is_some_and(|game| game.host.is_some()) {
                inner.redraw();
            }
        }
        self.changed();
    }

    fn spawn_refresh_device(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move { this.refresh_device().await });
    }

    /// The synchronous half of a read: Loading shows at once.
    fn begin_read(&self, keep: bool) -> Option<(Subject, u64)> {
        let started = {
            let mut inner = self.lock();
            let game = inner.game.clone()?;
            inner.generation += 1;
            if !keep {
                inner.snapshot = None;
                inner.list = None;
                inner.unreachable = false;
                inner.editor = Some(SurfaceLaunchSettings::Loading(
                    inner.content(vec![close_action()]),
                ));
            }
            (game, inner.generation)
        };
        self.changed();
        Some(started)
    }

    async fn read(self: &Arc<Self>, keep: bool) {
        if let Some((game, operation)) = self.begin_read(keep) {
            self.finish_read(game, operation).await;
        }
    }

    async fn finish_read(&self, game: Subject, operation: u64) {
        let result = self
            .snapshot(Some(game.id.clone()), game.host.clone())
            .await;
        {
            let mut inner = self.lock();
            if operation != inner.generation {
                return;
            }
            match result {
                Err(failure) => {
                    inner.snapshot = None;
                    inner.failed(&failure, Again::Read);
                }
                Ok(snapshot) => {
                    if game.host.is_none() {
                        inner.remember(&game.id, &snapshot);
                    }
                    inner.known.insert(game.key(), snapshot.clone());
                    inner.snapshot = Some(snapshot);
                    inner.again = None;
                    inner.ready();
                }
            }
        }
        self.changed();
    }

    fn spawn_read(self: &Arc<Self>, keep: bool) {
        if let Some((game, operation)) = self.begin_read(keep) {
            let this = Arc::clone(self);
            tokio::spawn(async move { this.finish_read(game, operation).await });
        }
    }

    /// One write to the device `host` names, or to this device.
    async fn send(
        &self,
        send: Write,
        snapshot: &Value,
        host: Option<String>,
    ) -> LaunchSettingsWriteOutcome {
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
                        expected_revision: revision(snapshot, &layer),
                        holder: host,
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
                    expected_revision: revision(snapshot, &json!({ "_tag": "Game" })),
                    holder: host,
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
                    expected_revision: revision(snapshot, &json!({ "_tag": "Game" })),
                    holder: host,
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
                    expected_revision: revision(snapshot, &json!({ "_tag": "Game" })),
                    holder: host,
                },
                LaunchSettingsWriteOutcome
            ),
        }
    }

    /// One write from the editor that is not a change: a profile's pick,
    /// name or removal. Saving shows on the row; the snapshot is read again
    /// after.
    fn write(self: &Arc<Self>, setting_id: String, send: Write) {
        let (snapshot, host, operation) = {
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
            let host = inner.game.as_ref().and_then(|game| game.host.clone());
            (snapshot, host, inner.generation)
        };
        self.changed();
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let result = this.send(send.clone(), &snapshot, host).await;
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
            let result = this.send(send, &snapshot, None).await;
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

    /// The values the next launch of this game carries: the changes kept
    /// for this launch only (ADR 0066). `game` is the game's key.
    pub(crate) fn submit_one_launch(
        &self,
        game: &str,
    ) -> (u64, Option<crate::launch_settings::LaunchOverride>) {
        self.lock().drafts.submit(game)
    }

    pub(crate) fn complete_one_launch(&self, ticket: u64, success: bool) {
        let mut inner = self.lock();
        inner.drafts.complete(ticket, success);
        if inner.editor_tag() == "Ready" {
            inner.ready();
        }
        drop(inner);
        self.changed();
    }

    fn problem(&self, setting_id: &str, message: impl Into<String>) {
        self.lock().status = Some(SurfaceSettingsStatus::Problem {
            setting_id: setting_id.into(),
            message: message.into(),
        });
        self.changed();
    }

    /// One list row changed. A value of the device that runs the game waits
    /// as a change; a value of this device's stream viewer is written now.
    fn change_row(self: &Arc<Self>, setting_id: &str, text: &str) -> bool {
        let (row, game) = {
            let inner = self.lock();
            let row = inner
                .list
                .as_ref()
                .and_then(|list| list.rows.get(setting_id))
                .cloned();
            (row, inner.game.clone())
        };
        let (Some(row), Some(game)) = (row, game) else {
            return false;
        };
        let Some(value) = row.value_of(text) else {
            self.problem(setting_id, INVALID);
            return true;
        };
        match row.target {
            RowTarget::Viewer(target) => {
                if value == row.stored {
                    return true;
                }
                let clear = value.is_null();
                self.write_device(
                    setting_id.into(),
                    Some(Write::Set {
                        target,
                        value: (!clear).then_some(value),
                        clear,
                    }),
                );
            }
            RowTarget::Holder { path, runner } => {
                // Korri knows a shared setting's shape before any device does.
                let shared = path["payload"]["key"]
                    .as_str()
                    .filter(|_| path["_tag"] == "Setting")
                    .and_then(korri_settings_treaty::SharedSetting::from_path);
                if let Some(shared) = shared.filter(|_| !value.is_null()) {
                    if let Err(message) =
                        crate::launch_settings::store::validate_property(&shared.property(), &value)
                    {
                        self.problem(setting_id, message);
                        return true;
                    }
                }
                let mut inner = self.lock();
                if inner.busy || inner.editor_tag() != "Ready" {
                    return true;
                }
                let key = game.key();
                if value == row.stored {
                    inner.drafts.remove(&key, setting_id);
                } else {
                    inner.drafts.set(
                        &key,
                        Change {
                            row: setting_id.into(),
                            path,
                            value,
                            runner,
                        },
                    );
                }
                inner.status = None;
                inner.ready();
                drop(inner);
                self.changed();
            }
        }
        true
    }

    /// Changes to a launch row. False: not a launch row.
    pub fn change(self: &Arc<Self>, setting_id: &str, value: &str) -> bool {
        if setting_id.starts_with("launch-device:") {
            self.change_device(setting_id, value);
            return true;
        }
        if setting_id == launch_id::KEEP {
            let mut inner = self.lock();
            let known = inner
                .list
                .as_ref()
                .is_some_and(|list| list.keep_places.contains_key(value));
            if let (true, Some(game)) = (known, inner.game.clone()) {
                inner.drafts.set_keep(&game.key(), value);
                inner.redraw();
                drop(inner);
                self.changed();
            }
            return true;
        }
        if self.change_row(setting_id, value) {
            return true;
        }
        let (game, snapshot) = {
            let inner = self.lock();
            (
                inner.game.as_ref().map(|game| game.id.clone()),
                inner.snapshot.clone(),
            )
        };
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
                self.problem(setting_id, "A profile needs a name.");
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
            self.spawn_close();
            return true;
        }
        if action_id == launch_id::RUNNER {
            let open = {
                let inner = self.lock();
                (inner.editor_tag() != "Closed")
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
                if inner.snapshot.is_some() && !matches!(again, Again::Read) {
                    inner.ready();
                }
                again
            };
            self.changed();
            match again {
                Again::Read => self.spawn_read(false),
                Again::Write(setting_id, send) => self.write(setting_id, *send),
                Again::Keep => self.spawn_close(),
            }
            return true;
        }
        let Some(profile_id) = action_id.strip_prefix("launch:profile:remove:") else {
            return false;
        };
        let (game_id, snapshot) = {
            let inner = self.lock();
            (
                inner.game.as_ref().map(|game| game.id.clone()),
                inner.snapshot.clone(),
            )
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

    /// The synchronous half of keeping: what to write and where. Changes for
    /// this launch are not written: they stay for the launch to carry.
    fn begin_keep(&self) -> Keep {
        let mut inner = self.lock();
        let Some(game) = inner.game.clone() else {
            return Keep::Nothing;
        };
        let key = game.key();
        let changes = inner.drafts.changes(&key).to_vec();
        if changes.is_empty() {
            return Keep::Nothing;
        }
        let layer = match &inner.list {
            Some(list) => list
                .keep_places
                .get(&list.keep_selected)
                .and_then(|place| place.layer.clone()),
            // No list to say which place is the default: only the person's
            // own choice of this launch counts.
            None if inner.drafts.keep(&key) == Some(KEEP_LAUNCH) => None,
            None => return Keep::Cannot,
        };
        let Some(layer) = layer else {
            return Keep::Nothing;
        };
        let snapshot = match inner.snapshot.clone() {
            Some(snapshot) if !inner.busy && inner.editor_tag() == "Ready" => snapshot,
            _ => return Keep::Cannot,
        };
        inner.generation += 1;
        inner.busy = true;
        inner.status = Some(SurfaceSettingsStatus::Saving {
            setting_id: launch_id::KEEP.into(),
        });
        Keep::Write(Keeping {
            game,
            layer,
            changes,
            snapshot,
            operation: inner.generation,
        })
    }

    /// Write the changes one by one, each from the revision the last left.
    /// A write that fails leaves the editor open, saying why; what was not
    /// written stays a change.
    async fn finish_keep(self: &Arc<Self>, keeping: Keeping) -> Closing {
        let Keeping {
            game,
            layer,
            changes,
            mut snapshot,
            operation,
        } = keeping;
        self.changed();
        let mut failure = None;
        for change in changes {
            // The governor is one device value: only the device keeps it.
            let layer = if change.path["_tag"] == "CpuGovernor" {
                json!({ "_tag": "Device" })
            } else {
                layer.clone()
            };
            // A shared value is kept for any runner, a runner-only value for its runner.
            let scope = match &change.runner {
                Some(runner) => json!({ "_tag": "Runner", "id": runner }),
                None => json!({ "_tag": "Any" }),
            };
            let clear = change.value.is_null();
            let result = self
                .send(
                    Write::Set {
                        target: LaunchWrite {
                            place: json!({ "layer": layer, "scope": scope }),
                            path: change.path.clone(),
                            encoder: super::launch_model::Encoder::Text,
                        },
                        value: (!clear).then(|| change.value.clone()),
                        clear,
                    },
                    &snapshot,
                    game.host.clone(),
                )
                .await;
            match result {
                LaunchSettingsWriteOutcome::Ok(revisions) => {
                    snapshot["revisions"] = serde_json::to_value(revisions).unwrap_or(Value::Null);
                    let mut inner = self.lock();
                    inner.drafts.kept(&game.key(), &change);
                    if operation == inner.generation {
                        if let Some(current) = inner.snapshot.as_mut() {
                            current["revisions"] = snapshot["revisions"].clone();
                        }
                    }
                }
                LaunchSettingsWriteOutcome::Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        {
            let mut inner = self.lock();
            inner.busy = false;
            inner.status = None;
            // The person closed or read the editor again meanwhile: it is
            // theirs now. What was written is still written.
            let current = operation == inner.generation;
            if let Some(failure) = failure.as_ref().filter(|_| current) {
                inner.failed(failure, Again::Keep);
            }
            if failure.is_some() || !current {
                drop(inner);
                self.changed();
                return match failure {
                    Some(_) => Closing::NotKept,
                    None => Closing::Kept,
                };
            }
        }
        self.cancel();
        self.spawn_refresh_device();
        Closing::Kept
    }

    /// The game starts: the changes are written to the place the person
    /// chose, then the editor closes (ADR 0066). The game may start only on
    /// `Kept`. An editor that cannot write its changes stays as it is.
    pub async fn close(self: &Arc<Self>) -> Closing {
        match self.begin_keep() {
            Keep::Write(keeping) => self.finish_keep(keeping).await,
            Keep::Nothing => {
                self.cancel();
                Closing::Kept
            }
            Keep::Cannot => Closing::NotKept,
        }
    }

    /// Close from an action: at once when there is nothing to write. An
    /// editor that cannot write leaves without writing; its changes stay
    /// with the game.
    fn spawn_close(self: &Arc<Self>) {
        match self.begin_keep() {
            Keep::Write(keeping) => {
                let this = Arc::clone(self);
                tokio::spawn(async move {
                    this.finish_keep(keeping).await;
                });
            }
            Keep::Nothing | Keep::Cannot => self.cancel(),
        }
    }

    /// Close without writing. Changes that are not kept stay with the game.
    pub fn cancel(&self) {
        {
            let mut inner = self.lock();
            inner.generation += 1;
            inner.game = None;
            inner.open_runners = None;
            inner.snapshot = None;
            inner.list = None;
            inner.unreachable = false;
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
            let result = this.snapshot(Some(id.clone()), None).await;
            let mut inner = this.lock();
            inner.learning.remove(&id);
            if let Ok(snapshot) = result {
                inner.remember(&id, &snapshot);
                drop(inner);
                this.changed();
            }
        });
    }

    /// Open the editor for one game, by korrid's id. `host` names the
    /// device that runs a streamed game.
    pub fn open(
        self: &Arc<Self>,
        id: String,
        title: String,
        host: Option<String>,
        choose_runners: Option<OpenRunners>,
    ) {
        let streamed = host.is_some();
        {
            let mut inner = self.lock();
            inner.game = Some(Subject { id, title, host });
            inner.open_runners = choose_runners;
        }
        if streamed {
            // Its list shows this device's stream section.
            self.spawn_refresh_device();
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
