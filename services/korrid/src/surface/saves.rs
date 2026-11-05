//! Port of `clients/portal/src/surface/save-sync.ts`: save sync on the host
//! side (ADR 0014). Each game's save mark, the save history, the split
//! choice, and the check with the owner's other devices before a launch.
//! korrid owns every rule; this store owns the questions, the waiting and the
//! words. korrid still refuses a launch over a split, so a skipped or failed
//! check cannot lose a save silently.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use korri_surface_treaty::{
    SurfaceAction, SurfaceSaveHistory, SurfaceSaveHistoryEntry, SurfaceSaveSide, SurfaceSaves,
    SurfaceStatus,
};

use super::{
    acquisition_gate::Step,
    korrid::{ask, Korrid},
    Notify,
};
use crate::{
    saves::types::{
        SaveCheck, SaveCheckOutcome, SaveCheckRequest, SaveGameRequest, SaveHistory,
        SaveHistoryEntry, SaveHistoryOutcome, SaveMark, SavePickRequest, SaveStatusOutcome,
        SaveStatusRequest,
    },
    PeerListOutcome, RpcFailure,
};

/// ADR 0014: the pre-launch wait for other devices is at most 3 seconds.
pub const SAVE_CHECK_LIMIT: Duration = Duration::from_millis(3000);
/// Marks change when sessions end and when devices meet.
pub const SAVE_STATUS_POLL: Duration = Duration::from_secs(15);
/// Game action that opens a game's save history.
pub const SAVE_HISTORY_ACTION_ID: &str = "save-history";
/// The Split mark's action: open the split choice without launching.
pub const SAVE_CHOOSE_ACTION_ID: &str = "save-choose";

/// The game a save question is about.
#[derive(Clone, Debug)]
pub struct SaveGameRef {
    pub id: String,
    pub surface_id: String,
    pub title: String,
    pub runner_id: Option<String>,
}

#[derive(Clone)]
struct Flow {
    game: SaveGameRef,
    /// Absent for a choice opened from the split review.
    proceed: Option<Step>,
    abandon: Step,
}

#[derive(Clone)]
enum Retry {
    ReadSides(Option<SaveCheck>, bool),
    RefreshThenReadSides(Option<SaveCheck>, bool),
    Pick(Vec<SaveHistoryEntry>, Option<SaveCheck>, bool, String),
}

#[derive(Clone)]
enum Phase {
    Idle,
    Checking(Flow),
    Reading(Flow),
    Choosing {
        flow: Flow,
        sides: Vec<SaveHistoryEntry>,
        check: Option<SaveCheck>,
        offline: bool,
    },
    Picking(Flow),
    Confirming(Flow, SaveCheck),
    Failed {
        flow: Flow,
        kicker: String,
        reason: String,
        retry: Option<Retry>,
    },
}

impl Phase {
    fn flow(&self) -> Option<&Flow> {
        match self {
            Phase::Idle => None,
            Phase::Checking(flow)
            | Phase::Reading(flow)
            | Phase::Picking(flow)
            | Phase::Confirming(flow, _)
            | Phase::Choosing { flow, .. }
            | Phase::Failed { flow, .. } => Some(flow),
        }
    }
}

#[derive(Clone)]
enum HistoryPhase {
    Closed,
    Loading(SaveGameRef),
    Error(SaveGameRef, String),
    Ready(SaveGameRef, SaveHistory),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveMarkFact {
    pub mark: SaveMark,
    pub heads: Vec<String>,
}

/// The treaty's save mark for one game, with the choose action on a split.
pub fn surface_saves_from(fact: Option<&SaveMarkFact>) -> Option<SurfaceSaves> {
    Some(match fact?.mark {
        SaveMark::Split => SurfaceSaves::Split {
            choose: SurfaceAction::new(SAVE_CHOOSE_ACTION_ID, "Choose a save", true),
        },
        SaveMark::Synced => SurfaceSaves::Synced,
        SaveMark::NotChecked => SurfaceSaves::NotChecked,
        SaveMark::NotSynced => SurfaceSaves::NotSynced,
        SaveMark::SyncedBySteam => SurfaceSaves::SyncedBySteam,
    })
}

/// History is meaningful only where Korri keeps snapshots.
pub fn offers_save_history(fact: Option<&SaveMarkFact>) -> bool {
    fact.is_some_and(|fact| !matches!(fact.mark, SaveMark::NotSynced | SaveMark::SyncedBySteam))
}

/// Plain words for a save failure. No code reaches the person.
pub fn save_failure_reason(failure: &RpcFailure) -> String {
    match failure.code.as_str() {
        "SavesSplit" => "This game's saves are split between two devices. Pick one save first.",
        "SaveContentUnavailable" => "This save's files are not on this device. Try again when the device that made it is in reach.",
        "SavePayloadUnavailable" => "That older save's files are not on any device in reach. Try again when the device that made it is on.",
        "ActiveSessionConflict" => "A game is running. Stop it, then try again.",
        "DeviceNeedsOwner" => "This device needs an owner before it can start a game.",
        "SavesUnavailable" => "Korri could not read or update the saves on this device.",
        "BrainUnreachable" => "Korri did not answer. Try again in a moment.",
        "PermissionDenied" => "This device cannot change saves.",
        "GameNotFound" => "Korri has no saves for this game.",
        _ => "Korri could not finish that. Try again in a moment.",
    }
    .into()
}

fn local_tm(epoch_seconds: i64) -> Option<libc::tm> {
    // SAFETY: localtime_r writes only into the provided struct.
    unsafe {
        let time = epoch_seconds as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        (!libc::localtime_r(&time, &mut tm).is_null()).then_some(tm)
    }
}

/// "4:02 PM" today, else "Oct 12, 4:02 PM", in local time.
pub fn format_since(since_millis: u64, now_millis: u64) -> String {
    let (Some(date), Some(now)) = (
        local_tm((since_millis / 1000) as i64),
        local_tm((now_millis / 1000) as i64),
    ) else {
        return String::new();
    };
    let hour = date.tm_hour % 12;
    let time = format!(
        "{}:{:02} {}",
        if hour == 0 { 12 } else { hour },
        date.tm_min,
        if date.tm_hour < 12 { "AM" } else { "PM" }
    );
    if (date.tm_year, date.tm_yday) == (now.tm_year, now.tm_yday) {
        return time;
    }
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{} {}, {time}",
        MONTHS[date.tm_mon.clamp(0, 11) as usize],
        date.tm_mday
    )
}

fn ended_millis(entry: &SaveHistoryEntry) -> f64 {
    chrono::DateTime::parse_from_rfc3339(&entry.record.ended_at)
        .map_or(0.0, |time| time.timestamp_millis() as f64)
}

/// Newest first by ancestry: a snapshot always comes before the snapshots
/// it started from. Among snapshots ancestry does not order, the later end
/// time comes first; that is display order only.
pub fn newest_first(records: &[SaveHistoryEntry]) -> Vec<SaveHistoryEntry> {
    let by_id: HashMap<&str, &SaveHistoryEntry> = records
        .iter()
        .map(|entry| (entry.snapshot_id.as_str(), entry))
        .collect();
    let mut children: HashMap<String, i64> = HashMap::new();
    for entry in records {
        for parent in &entry.record.parents {
            if by_id.contains_key(parent.as_str()) {
                *children.entry(parent.clone()).or_default() += 1;
            }
        }
    }
    let mut ready: Vec<&SaveHistoryEntry> = records
        .iter()
        .filter(|entry| children.get(&entry.snapshot_id).copied().unwrap_or(0) == 0)
        .collect();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    while !ready.is_empty() {
        ready.sort_by(|a, b| {
            ended_millis(b)
                .partial_cmp(&ended_millis(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let next = ready.remove(0);
        if !seen.insert(next.snapshot_id.clone()) {
            continue;
        }
        out.push(next.clone());
        for parent in &next.record.parents {
            let left = children.get(parent).copied().unwrap_or(0) - 1;
            children.insert(parent.clone(), left);
            if let (0, Some(entry)) = (left, by_id.get(parent.as_str())) {
                ready.push(entry);
            }
        }
    }
    // A cycle would be korrid's defect; never drop a snapshot because of it.
    for entry in records {
        if !seen.contains(&entry.snapshot_id) {
            out.push(entry.clone());
        }
    }
    out
}

pub type LocalKeys = Arc<dyn Fn() -> HashSet<String> + Send + Sync>;

struct Inner {
    phase: Phase,
    history_phase: HistoryPhase,
    status: Option<SurfaceStatus>,
    history_view: Option<SurfaceSaveHistory>,
    marks: BTreeMap<String, SaveMarkFact>,
    base: BTreeMap<String, SaveMarkFact>,
    checked: HashMap<String, SaveMarkFact>,
    peers: HashMap<String, String>,
    generation: u64,
    history_generation: u64,
}

pub struct SaveSync {
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
    local_keys: LocalKeys,
    check_limit: Duration,
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

fn same_heads(a: &[String], b: &[String]) -> bool {
    let mut a = a.to_vec();
    let mut b = b.to_vec();
    a.sort();
    b.sort();
    a == b
}

impl Inner {
    fn device_name(&self, key: &str, local: &HashSet<String>) -> String {
        if local.contains(key) {
            "This device".into()
        } else {
            self.peers
                .get(key)
                .cloned()
                .unwrap_or_else(|| "Another device".into())
        }
    }

    fn action(&self, id: &str, label: &str) -> SurfaceAction {
        SurfaceAction::new(format!("saves:{}:{id}", self.generation), label, true)
    }

    fn history_action(&self, id: &str, label: &str) -> SurfaceAction {
        SurfaceAction::new(
            format!("saves:h{}:{id}", self.history_generation),
            label,
            true,
        )
    }

    fn confirm_copy(&self, check: &SaveCheck, local: &HashSet<String>) -> (String, String) {
        let now = now_millis();
        let warnings = check.warnings.clone();
        if let Some((first, more)) = check.in_use.split_first() {
            let mut detail =
                vec!["If you play here too, Korri asks which save to keep afterwards.".to_owned()];
            detail.extend(more.iter().map(|other| {
                format!(
                    "Also in use on {} since {}.",
                    self.device_name(&other.device, local),
                    format_since(other.since, now)
                )
            }));
            detail.extend(warnings);
            return (
                format!(
                    "In use on {} since {}",
                    self.device_name(&first.device, local),
                    format_since(first.since, now)
                ),
                detail.join(" "),
            );
        }
        ("Saves may not load".into(), warnings.join(" "))
    }

    fn status_for(&self, local: &HashSet<String>) -> Option<SurfaceStatus> {
        let game_id = self.phase.flow().map(|flow| flow.game.surface_id.clone());
        // Back closed the flow while korrid checked or read: a marked Cancel.
        let cancel = self.action("cancel", "Cancel");
        let busy =
            |kicker: &str, mut actions: Vec<SurfaceAction>, back: bool| SurfaceStatus::Busy {
                kicker: kicker.into(),
                detail: None,
                progress: None,
                steps: None,
                decline_action_id: back.then(|| cancel.id.clone()),
                actions: {
                    if back {
                        actions.push(cancel.clone());
                    }
                    (!actions.is_empty()).then_some(actions)
                },
                game_id: game_id.clone(),
            };
        Some(match &self.phase {
            Phase::Idle => return None,
            Phase::Checking(_) => busy("Checking other devices…", vec![self.action("skip", "Skip")], true),
            Phase::Reading(_) => busy("Reading both saves…", Vec::new(), true),
            // The pick is a korrid write under way: Back cancelled nothing.
            Phase::Picking(_) => busy("Continuing from that save…", Vec::new(), false),
            Phase::Choosing { sides, offline, .. } => SurfaceStatus::ChooseSave {
                decline_action_id: Some(self.action("not-now", "Not now").id),
                game_id: game_id.clone().unwrap_or_default(),
                kicker: "Saves split between two devices".into(),
                detail: offline.then(|| "No other device answered. Your pick syncs to them when they are next in reach.".into()),
                // Exactly one action besides the sides, so Back always has a meaning.
                actions: vec![self.action("not-now", "Not now")],
                sides: sides
                    .iter()
                    .map(|entry| SurfaceSaveSide {
                        action_id: self.action(&format!("pick:{}", entry.snapshot_id), "").id,
                        device: self.device_name(&entry.record.device, local),
                        ended_at: ended_millis(entry),
                        duration_seconds: entry.record.duration_seconds as f64,
                        image_url: None,
                    })
                    .collect(),
            },
            Phase::Confirming(_, check) => {
                let (kicker, detail) = self.confirm_copy(check, local);
                SurfaceStatus::Confirm {
                    kicker,
                    detail: (!detail.is_empty()).then_some(detail),
                    actions: vec![self.action("launch-anyway", "Launch anyway"), self.action("cancel", "Cancel")],
                    decline_action_id: Some(self.action("cancel", "Cancel").id),
                    game_id,
                }
            }
            Phase::Failed { flow, kicker, reason, retry } => SurfaceStatus::Problem {
                kicker: kicker.clone(),
                reason: reason.clone(),
                can_retry: retry.is_some(),
                game_id,
                game_title: Some(flow.game.title.clone()),
            },
        })
    }

    fn history_for(&self, local: &HashSet<String>) -> Option<SurfaceSaveHistory> {
        // Exactly one host action, so Back always closes.
        let actions = vec![self.history_action("close", "Close")];
        Some(match &self.history_phase {
            HistoryPhase::Closed => return None,
            HistoryPhase::Loading(game) => SurfaceSaveHistory::Loading { game_id: game.surface_id.clone(), actions },
            HistoryPhase::Error(game, message) => SurfaceSaveHistory::Error {
                game_id: game.surface_id.clone(),
                message: message.clone(),
                actions,
            },
            HistoryPhase::Ready(game, history) => SurfaceSaveHistory::Ready {
                game_id: game.surface_id.clone(),
                actions,
                entries: newest_first(&history.records)
                    .iter()
                    .map(|entry| {
                        let current = Some(&entry.snapshot_id) == history.current.as_ref();
                        SurfaceSaveHistoryEntry {
                            snapshot_id: entry.snapshot_id.clone(),
                            device: self.device_name(&entry.record.device, local),
                            ended_at: ended_millis(entry),
                            duration_seconds: entry.record.duration_seconds as f64,
                            interrupted: entry.record.interrupted,
                            current,
                            // A restore replaces what the game plays from, so
                            // every surface asks first.
                            restore: (!current).then(|| {
                                self.history_action(&format!("restore:{}", entry.snapshot_id), "Restore")
                                    .destructive()
                                    .with_description("Makes a new save from this one. The save you have now stays in the history.")
                            }),
                        }
                    })
                    .collect(),
            },
        })
    }

    /// korrid's marks with this device's last check laid over them.
    fn merged(&mut self) {
        let mut out = self.base.clone();
        let checked: Vec<(String, SaveMarkFact)> = self
            .checked
            .iter()
            .map(|(id, fact)| (id.clone(), fact.clone()))
            .collect();
        for (game_id, check) in checked {
            let fact = out.get(&game_id);
            if fact.is_some_and(|fact| {
                fact.mark == SaveMark::Split || !same_heads(&fact.heads, &check.heads)
            }) {
                self.checked.remove(&game_id);
                continue;
            }
            let heads = fact.map_or(check.heads.clone(), |fact| fact.heads.clone());
            out.insert(
                game_id,
                SaveMarkFact {
                    mark: check.mark,
                    heads,
                },
            );
        }
        self.marks = out;
    }
}

impl SaveSync {
    pub fn new(
        korrid: Korrid,
        notify: Notify,
        local_keys: LocalKeys,
        check_limit: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            korrid,
            inner: Mutex::new(Inner {
                phase: Phase::Idle,
                history_phase: HistoryPhase::Closed,
                status: None,
                history_view: None,
                marks: BTreeMap::new(),
                base: BTreeMap::new(),
                checked: HashMap::new(),
                peers: HashMap::new(),
                generation: 0,
                history_generation: 0,
            }),
            notify,
            local_keys,
            check_limit,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("save sync poisoned")
    }

    pub fn marks(&self) -> BTreeMap<String, SaveMarkFact> {
        self.lock().marks.clone()
    }
    pub fn status(&self) -> Option<SurfaceStatus> {
        self.lock().status.clone()
    }
    pub fn history(&self) -> Option<SurfaceSaveHistory> {
        self.lock().history_view.clone()
    }
    pub fn has_history(&self, game_id: &str) -> bool {
        offers_save_history(self.lock().base.get(game_id))
    }

    fn render(&self) {
        let local = (self.local_keys)();
        {
            let mut inner = self.lock();
            inner.status = inner.status_for(&local);
            inner.history_view = inner.history_for(&local);
        }
        (self.notify)();
    }

    fn publish(&self, next: Phase) {
        self.lock().phase = next;
        self.render();
    }

    fn publish_history(&self, next: HistoryPhase) {
        self.lock().history_phase = next;
        self.render();
    }

    fn bump(&self) -> u64 {
        let mut inner = self.lock();
        inner.generation += 1;
        inner.generation
    }

    fn current(&self, operation: u64) -> bool {
        self.lock().generation == operation
    }

    /// Re-read every mark and the peer names; then every 15 s.
    pub fn start(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(SAVE_STATUS_POLL).await;
                this.refresh().await;
            }
        });
    }

    pub async fn refresh(&self) {
        let (status, peers) = tokio::join!(
            async {
                ask!(
                    self.korrid,
                    Route,
                    SavesStatus,
                    SaveStatusRequest {},
                    SaveStatusOutcome
                )
            },
            self.korrid.peer_list()
        );
        {
            let mut inner = self.lock();
            if let PeerListOutcome::Ok(list) = peers {
                inner.peers = list
                    .peers
                    .into_iter()
                    .map(|peer| (peer.device_public_key, peer.label))
                    .collect();
            }
            // A failed read states nothing: no mark rather than a guess.
            inner.base = match status {
                SaveStatusOutcome::Ok(status) => status
                    .games
                    .into_iter()
                    .map(|game| {
                        (
                            game.game_id,
                            SaveMarkFact {
                                mark: game.status,
                                heads: game.heads,
                            },
                        )
                    })
                    .collect(),
                SaveStatusOutcome::Err(_) => BTreeMap::new(),
            };
            inner.merged();
        }
        self.render();
    }

    fn changed(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move { this.refresh().await });
    }

    fn close(&self, flow: &Flow) {
        self.bump();
        self.publish(Phase::Idle);
        (flow.abandon)();
    }

    fn finish(&self, flow: &Flow) {
        self.bump();
        self.publish(Phase::Idle);
        if let Some(proceed) = &flow.proceed {
            proceed();
        }
    }

    fn note_checked(&self, game_id: &str, mark: SaveMark) {
        {
            let mut inner = self.lock();
            let heads = inner
                .base
                .get(game_id)
                .map(|fact| fact.heads.clone())
                .unwrap_or_default();
            inner
                .checked
                .insert(game_id.into(), SaveMarkFact { mark, heads });
            inner.merged();
        }
        self.render();
    }

    fn failed(&self, flow: Flow, kicker: &str, reason: String, retry: Option<Retry>) {
        self.bump();
        self.publish(Phase::Failed {
            flow,
            kicker: kicker.into(),
            reason,
            retry,
        });
    }

    /// After the split (or none): ask about in-use and warnings, then go.
    fn after_split(&self, flow: Flow, check: Option<SaveCheck>) {
        if let Some(check) = check.filter(|check| {
            flow.proceed.is_some() && (!check.in_use.is_empty() || !check.warnings.is_empty())
        }) {
            self.bump();
            self.publish(Phase::Confirming(flow, check));
            return;
        }
        if flow.proceed.is_none() {
            self.close(&flow);
            return;
        }
        self.finish(&flow);
    }

    /// Read the history, find both sides by the split's heads, and ask.
    async fn read_sides(self: Arc<Self>, flow: Flow, check: Option<SaveCheck>, offline: bool) {
        let operation = self.bump();
        self.publish(Phase::Reading(flow.clone()));
        let result = ask!(
            self.korrid,
            Route,
            SavesHistory,
            SaveGameRequest {
                game_id: flow.game.id.clone()
            },
            SaveHistoryOutcome
        );
        if !self.current(operation) {
            return;
        }
        let history = match result {
            SaveHistoryOutcome::Err(failure) => {
                self.failed(
                    flow,
                    "Couldn't read the saves",
                    save_failure_reason(&failure),
                    Some(Retry::ReadSides(check, offline)),
                );
                return;
            }
            SaveHistoryOutcome::Ok(history) => history,
        };
        let heads = self
            .lock()
            .marks
            .get(&flow.game.id)
            .map(|fact| fact.heads.clone())
            .unwrap_or_default();
        let sides: Vec<SaveHistoryEntry> = if heads.len() > 1 {
            newest_first(&history.records)
                .into_iter()
                .filter(|entry| heads.contains(&entry.snapshot_id))
                .collect()
        } else {
            Vec::new()
        };
        if sides.len() < 2 {
            self.failed(
                flow,
                "Couldn't read both saves",
                "Korri found a split but not both of its saves. Try again.".into(),
                Some(Retry::RefreshThenReadSides(check, offline)),
            );
            return;
        }
        self.bump();
        self.publish(Phase::Choosing {
            flow,
            sides,
            check,
            offline,
        });
    }

    async fn check(self: Arc<Self>, flow: Flow) {
        let operation = self.bump();
        self.publish(Phase::Checking(flow.clone()));
        let answer = tokio::time::timeout(self.check_limit, async {
            ask!(
                self.korrid,
                Route,
                SavesCheck,
                SaveCheckRequest {
                    game_id: flow.game.id.clone(),
                    runner_id: flow.game.runner_id.clone()
                },
                SaveCheckOutcome
            )
        })
        .await;
        if !self.current(operation) || !matches!(self.lock().phase, Phase::Checking(_)) {
            return;
        }
        let result = match answer {
            Ok(SaveCheckOutcome::Ok(result)) => result,
            _ => {
                self.unanswered(flow);
                return;
            }
        };
        self.note_checked(&flow.game.id, result.status.clone());
        if result.status == SaveMark::Split {
            self.refresh().await;
            if !self.current(operation) {
                return;
            }
            tokio::spawn(Arc::clone(&self).read_sides(flow, Some(result), false));
            return;
        }
        self.after_split(flow, Some(result));
    }

    /// No device answered, the check failed, or the person skipped it.
    fn unanswered(self: &Arc<Self>, flow: Flow) {
        self.note_checked(&flow.game.id, SaveMark::NotChecked);
        // A split korrid already knows still needs a choice.
        let split = self
            .lock()
            .marks
            .get(&flow.game.id)
            .is_some_and(|fact| fact.mark == SaveMark::Split);
        if split {
            tokio::spawn(Arc::clone(self).read_sides(flow, None, true));
            return;
        }
        self.finish(&flow);
    }

    async fn pick(
        self: Arc<Self>,
        flow: Flow,
        sides: Vec<SaveHistoryEntry>,
        check: Option<SaveCheck>,
        offline: bool,
        snapshot_id: String,
    ) {
        let operation = self.bump();
        self.publish(Phase::Picking(flow.clone()));
        let result = ask!(
            self.korrid,
            Route,
            SavesChoose,
            SavePickRequest {
                game_id: flow.game.id.clone(),
                snapshot_id: snapshot_id.clone()
            },
            SaveHistoryOutcome
        );
        if !self.current(operation) {
            return;
        }
        if let SaveHistoryOutcome::Err(failure) = result {
            self.failed(
                flow,
                "Couldn't use that save",
                save_failure_reason(&failure),
                Some(Retry::Pick(sides, check, offline, snapshot_id)),
            );
            return;
        }
        self.lock().checked.remove(&flow.game.id);
        self.changed();
        self.after_split(flow, check);
    }

    async fn load_history(self: Arc<Self>, game: SaveGameRef) {
        let operation = {
            let mut inner = self.lock();
            inner.history_generation += 1;
            inner.history_generation
        };
        self.publish_history(HistoryPhase::Loading(game.clone()));
        let result = ask!(
            self.korrid,
            Route,
            SavesHistory,
            SaveGameRequest {
                game_id: game.id.clone()
            },
            SaveHistoryOutcome
        );
        if self.lock().history_generation != operation {
            return;
        }
        self.publish_history(match result {
            SaveHistoryOutcome::Ok(history) => HistoryPhase::Ready(game, history),
            SaveHistoryOutcome::Err(failure) => {
                HistoryPhase::Error(game, save_failure_reason(&failure))
            }
        });
    }

    async fn restore(self: Arc<Self>, game: SaveGameRef, snapshot_id: String) {
        let operation = {
            let mut inner = self.lock();
            inner.history_generation += 1;
            inner.history_generation
        };
        self.publish_history(HistoryPhase::Loading(game.clone()));
        let result = ask!(
            self.korrid,
            Route,
            SavesRestore,
            SavePickRequest {
                game_id: game.id.clone(),
                snapshot_id
            },
            SaveHistoryOutcome
        );
        if self.lock().history_generation != operation {
            return;
        }
        match result {
            SaveHistoryOutcome::Err(failure) => self.publish_history(HistoryPhase::Error(
                game,
                format!(
                    "Couldn't restore that save. {}",
                    save_failure_reason(&failure)
                ),
            )),
            SaveHistoryOutcome::Ok(history) => {
                self.publish_history(HistoryPhase::Ready(game, history));
                self.changed();
            }
        }
    }

    fn idle(&self) -> bool {
        matches!(self.lock().phase, Phase::Idle)
    }

    /// Check, ask, then `proceed` into the launch; `abandon` when it does not.
    pub fn launch(self: &Arc<Self>, game: SaveGameRef, proceed: Step, abandon: Step) {
        if !self.idle() {
            return;
        }
        tokio::spawn(Arc::clone(self).check(Flow {
            game,
            proceed: Some(proceed),
            abandon,
        }));
    }

    /// The split choice for one game, from the review: no launch follows.
    pub fn choose(self: &Arc<Self>, game: SaveGameRef) {
        if !self.idle() {
            return;
        }
        tokio::spawn(Arc::clone(self).read_sides(
            Flow {
                game,
                proceed: None,
                abandon: Arc::new(|| {}),
            },
            None,
            false,
        ));
    }

    /// korrid refused a launch over a split (SavesSplit): ask, then proceed.
    pub fn resolve_split(self: &Arc<Self>, game: SaveGameRef, proceed: Step, abandon: Step) {
        if !self.idle() {
            return;
        }
        let flow = Flow {
            game,
            proceed: Some(proceed),
            abandon,
        };
        let operation = self.bump();
        self.publish(Phase::Reading(flow.clone()));
        let this = Arc::clone(self);
        tokio::spawn(async move {
            // Re-read the heads korrid just refused on, then ask.
            this.refresh().await;
            if this.current(operation) {
                Arc::clone(&this).read_sides(flow, None, false).await;
            }
        });
    }

    /// korrid refused a start for a reason the person must fix first.
    pub fn refuse_start(&self, game: SaveGameRef, failure: &RpcFailure, abandon: Step) {
        if !self.idle() {
            return;
        }
        let kicker = format!("Couldn't start {}", game.title);
        self.failed(
            Flow {
                game,
                proceed: None,
                abandon,
            },
            &kicker,
            save_failure_reason(failure),
            None,
        );
    }

    pub fn open_history(self: &Arc<Self>, game: SaveGameRef) {
        tokio::spawn(Arc::clone(self).load_history(game));
    }

    fn published(&self, id: &str) -> bool {
        let inner = self.lock();
        let mut candidates: Vec<SurfaceAction> = Vec::new();
        match &inner.status {
            Some(SurfaceStatus::Busy {
                actions: Some(actions),
                ..
            })
            | Some(SurfaceStatus::Confirm { actions, .. }) => {
                candidates.extend(actions.iter().cloned())
            }
            Some(SurfaceStatus::ChooseSave { actions, sides, .. }) => {
                candidates.extend(actions.iter().cloned());
                candidates.extend(
                    sides
                        .iter()
                        .map(|side| SurfaceAction::new(side.action_id.clone(), "", true)),
                );
            }
            _ => {}
        }
        match &inner.history_view {
            Some(SurfaceSaveHistory::Ready {
                actions, entries, ..
            }) => {
                candidates.extend(actions.iter().cloned());
                candidates.extend(entries.iter().filter_map(|entry| entry.restore.clone()));
            }
            Some(
                SurfaceSaveHistory::Loading { actions, .. }
                | SurfaceSaveHistory::Error { actions, .. },
            ) => candidates.extend(actions.iter().cloned()),
            None => {}
        }
        candidates
            .iter()
            .any(|candidate| candidate.id == id && candidate.enabled)
    }

    /// A published `saves:` action id. True when consumed.
    pub fn act(self: &Arc<Self>, action_id: &str) -> bool {
        let Some(after) = action_id.strip_prefix("saves:") else {
            return false;
        };
        if !self.published(action_id) {
            return true;
        }
        let rest = after[after.find(':').map_or(0, |at| at + 1)..].to_owned();
        if after.starts_with('h') {
            let phase = self.lock().history_phase.clone();
            match phase {
                HistoryPhase::Closed => {}
                _ if rest == "close" => {
                    self.lock().history_generation += 1;
                    self.publish_history(HistoryPhase::Closed);
                }
                HistoryPhase::Ready(game, _) => {
                    if let Some(snapshot_id) = rest.strip_prefix("restore:") {
                        tokio::spawn(Arc::clone(self).restore(game, snapshot_id.to_owned()));
                    }
                }
                _ => {}
            }
            return true;
        }
        let phase = self.lock().phase.clone();
        match phase {
            // Skip ends the wait here; korrid still enforces the split.
            Phase::Checking(flow) | Phase::Reading(flow) if rest == "cancel" => self.close(&flow),
            Phase::Checking(flow) if rest == "skip" => {
                self.bump();
                self.unanswered(flow);
            }
            Phase::Choosing {
                flow,
                sides,
                check,
                offline,
            } => {
                if rest == "not-now" {
                    self.close(&flow);
                } else if let Some(snapshot_id) = rest.strip_prefix("pick:") {
                    tokio::spawn(Arc::clone(self).pick(
                        flow,
                        sides,
                        check,
                        offline,
                        snapshot_id.to_owned(),
                    ));
                }
            }
            Phase::Confirming(flow, _) => {
                if rest == "launch-anyway" {
                    self.finish(&flow);
                } else if rest == "cancel" {
                    self.close(&flow);
                }
            }
            _ => {}
        }
        true
    }

    /// The host's Back while a question is open. True when consumed.
    pub fn back(&self) -> bool {
        let phase = self.lock().phase.clone();
        match phase {
            Phase::Idle => false,
            Phase::Picking(_) => true,
            other => {
                if let Some(flow) = other.flow() {
                    self.close(flow);
                }
                true
            }
        }
    }

    pub fn retry(self: &Arc<Self>) -> bool {
        let Phase::Failed { flow, retry, .. } = self.lock().phase.clone() else {
            return false;
        };
        match retry {
            None => {}
            Some(Retry::ReadSides(check, offline)) => {
                tokio::spawn(Arc::clone(self).read_sides(flow, check, offline));
            }
            Some(Retry::RefreshThenReadSides(check, offline)) => {
                let this = Arc::clone(self);
                tokio::spawn(async move {
                    this.refresh().await;
                    Arc::clone(&this).read_sides(flow, check, offline).await;
                });
            }
            Some(Retry::Pick(sides, check, offline, snapshot_id)) => {
                tokio::spawn(Arc::clone(self).pick(flow, sides, check, offline, snapshot_id));
            }
        }
        true
    }

    pub fn dismiss(&self) -> bool {
        let Phase::Failed { flow, .. } = self.lock().phase.clone() else {
            return false;
        };
        self.close(&flow);
        true
    }
}
