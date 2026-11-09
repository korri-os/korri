//! Korri surface treaty — the boundary between Korri and a presentation surface.
//!
//! A surface is a self-contained front end. It renders what Korri can
//! currently do and asks Korri to do things; it never reaches into korrid,
//! the Linux shell, portal state, or any other Rust type. That one-way
//! dependency is what lets a surface live in its own repository — or ship as
//! an installable package — without dragging Korri with it.
//!
//! Rules this crate exists to hold:
//!   - Korri hands the surface a MODEL (what is true right now). The surface
//!     returns REQUESTS (what it asks for). Nothing else crosses.
//!   - The model carries only facts Korri can actually produce today. A surface
//!     may render less than the model offers, never more than Korri knows.
//!   - Absent data is absent, never invented. A surface decides how to present
//!     the gap, such as a title in place of cover art.
//!   - Requests are fire-and-forget. Korri publishes the consequence as a new
//!     model; the surface never awaits a result or holds Korri's state.
//!
//! Product language: see ../../CONTEXT.md. Its words guide surface authors;
//! each independently authored surface owns its own text.
//!
//! Typeshare writes `contracts/generated/surface.ts` from this crate. Tagged
//! unions keep the existing treaty's JSON (`_tag` or `kind` inside the
//! object), which Typeshare cannot derive, so their TypeScript spelling lives
//! in `typeshare.toml` beside the serde attributes that enforce it here.
//!
//! This crate imports nothing from Korri. It is the whole contract.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use typeshare::typeshare;

// ---------------------------------------------------------------------------
// Literal helpers
// ---------------------------------------------------------------------------

macro_rules! literal {
    ($(#[$meta:meta])* $name:ident: $ty:ty = $value:expr) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
        pub struct $name;
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                $value.serialize(serializer)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = <$ty as Deserialize>::deserialize(deserializer)?;
                if value == $value {
                    Ok($name)
                } else {
                    Err(serde::de::Error::custom(concat!("expected ", stringify!($value))))
                }
            }
        }
    };
}

literal!(
    /// The literal `true`: a flag that is either present and true, or absent.
    SurfaceTrue: bool = true
);
literal!(
    /// The literal `"gameplay-overlay"` of `SurfaceGameplayOverlayPresentation.kind`.
    SurfaceGameplayOverlayKind: String = "gameplay-overlay"
);
literal!(
    /// The literal `"catalog"` of `SurfaceCatalogPresentation.kind`.
    SurfaceCatalogKind: String = "catalog"
);

// ---------------------------------------------------------------------------
// Games
// ---------------------------------------------------------------------------

/// One place that can launch a game. `id` is opaque to the surface — it is
/// handed back verbatim when asking Korri to launch on it.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceLaunchLocation {
    /// Opaque copy id. A surface returns it unchanged when the user chooses.
    pub id: String,
    /// Human-readable host name, such as "This device" or "zao".
    pub label: String,
}

/// Measured progress, in bytes. A surface draws a real bar from it. `total` is
/// absent when Korri knows how far it has got but not how far it goes.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceProgress {
    pub done: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
}

/// A launch's steps, named, in order. `labels` lists only the steps that run
/// on this launch, in plain user-facing words. `current` is the zero-based
/// index of the step running now. Korri fixes the list when the launch
/// begins. The only amendment is at Start: when a peer's runner steps were
/// unknown at plan time, they replace the single "Starting the game" step
/// exactly once. No other growth is allowed. A step that turns out to be
/// unneeded completes at once. The current index never moves back. A surface
/// draws one segment per step.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSteps {
    pub labels: Vec<String>,
    pub current: u32,
}

/// Where one download stands, wherever it shows: a game's own download and a
/// store entry being got. Korri fetches one thing at a time, so a download can
/// wait its turn. Surfaces word both facts themselves.
#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceDownloadState {
    /// Waiting behind another download.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued: Option<SurfaceTrue>,
    /// While it downloads. No `total` means no known end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<SurfaceProgress>,
}

/// `SurfaceGame.download`: `SurfaceDownloadState & { bytes?: number }`.
#[typeshare(serialized_as = "SurfaceGameDownloadWire")]
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceGameDownload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued: Option<SurfaceTrue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<SurfaceProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<f64>,
}

/// Save-sync facts for this person and game.
#[typeshare(serialized_as = "SurfaceSavesWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag")]
pub enum SurfaceSaves {
    Synced,
    NotChecked,
    NotSynced,
    SyncedBySteam,
    Split { choose: SurfaceAction },
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceGame {
    pub id: String,
    pub title: String,
    /// Places that can launch this game. Present only when there is a real
    /// choice; surfaces must ask the user rather than silently selecting or
    /// falling back. Korri orders the local device first, followed by stable
    /// remote host order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_locations: Option<Vec<SurfaceLaunchLocation>>,
    /// Grouping caption ("Continue", "This device", a peer device's name).
    /// Games sharing a section arrive consecutively; the surface groups on
    /// change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Short provenance line, e.g. "GBA · This device".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Cover art. Absent when Korri has none — surfaces must have a fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_art_url: Option<String>,
    /// True when confirming continues an existing session rather than starting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumable: Option<bool>,
    /// The title of the launch profile the next launch of this game uses, when
    /// Korri has read it and one is picked. Surfaces show it beside Play.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_profile: Option<String>,
    /// Newest recorded session end, as UTC epoch milliseconds. Absent when
    /// Korri has never recorded a session for this game. Surfaces format it;
    /// Korri never ships a label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played_at: Option<f64>,
    /// Recorded sessions. Absent together with lastPlayedAt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_count: Option<u32>,
    /// Summed recorded session length. Absent together with lastPlayedAt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_playtime_seconds: Option<f64>,
    /// Save-sync facts for this person and game. Absent until Korri knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saves: Option<SurfaceSaves>,
    /// Present when this game needs anything downloaded to this device: its
    /// own files, or a runtime it depends on. `bytes` is the total still to
    /// fetch; absent when Korri does not know the size. `queued` and
    /// `progress` say where the download stands (`SurfaceDownloadState`).
    /// Surfaces format all of it; Korri never ships a label. Absent when
    /// nothing is needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download: Option<SurfaceGameDownload>,
}

/// A non-game thing the user can do (open settings, cancel a launch, end
/// the running game). Korri owns which of these exist and whether they apply;
/// the surface only renders and reports selection.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceAction {
    pub id: String,
    pub label: String,
    /// One-line explanation. Surfaces may show it or not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Present but inert. Surfaces render it dimmed rather than hiding it.
    pub enabled: bool,
    /// Destructive/irreversible. Surfaces may mark it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive: Option<bool>,
}

impl SurfaceAction {
    pub fn new(id: impl Into<String>, label: impl Into<String>, enabled: bool) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            enabled,
            destructive: None,
        }
    }
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
    pub fn destructive(mut self) -> Self {
        self.destructive = Some(true);
        self
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// One value offered by a choice setting. Values are opaque to the surface.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSettingChoice {
    pub value: String,
    pub label: String,
}

/// The question a surface asks before it fires a destructive action.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSettingConfirmation {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
}

/// How a user may interact with a setting. Absent means read-only.
///
/// - `action`: `actionId` is a setting-local command id; pass it to
///   `RunAction` unchanged. Destructive actions should be confirmed by the
///   surface before firing.
/// - `choice`: `selected` is the `value` of the choice in force. Two choices
///   may share a label (two profiles named "Quiet"), so a surface marks and
///   steps from this value, never by matching `SurfaceSettingItem.value` to a
///   label.
/// - `number`: a number typed or stepped. `value` is the number set on this
///   row; absent when the row has none of its own. The surface sends the new
///   number as decimal text. When `clearable`, `ChangeSetting(id, "")` clears
///   the row's own value, so the value in force comes from elsewhere again.
/// - `text`: `allowEmpty` present means an empty text is a value Korri takes:
///   the surface may send "". Absent: Korri refuses an empty text, so a
///   surface need not offer it.
/// - `sensitiveText`: a secret Korri never republishes.
#[typeshare(serialized_as = "SurfaceSettingInteractionWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SurfaceSettingInteraction {
    Action {
        action_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        destructive: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confirmation: Option<SurfaceSettingConfirmation>,
    },
    Choice {
        choices: Vec<SurfaceSettingChoice>,
        selected: String,
    },
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clearable: Option<bool>,
    },
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_length: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        allow_empty: Option<bool>,
    },
    SensitiveText {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_length: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clear_label: Option<String>,
    },
}

/// Whether a value stays on this device (`device`, shown as "device-only") or
/// may follow the person to other devices (`shared`).
#[typeshare(serialized_as = "SurfaceSettingScopeWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceSettingScope {
    Device,
    Shared,
}

/// One device fact or setting. Korri owns truth and allowed interactions.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSettingItem {
    pub id: String,
    pub label: String,
    /// Current state as display text: "usu", "Granted", "2 devices".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// One-line explanation. Surfaces may show it or not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Absent means this row is genuinely read-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction: Option<SurfaceSettingInteraction>,
    /// Where the value in force comes from, as display text: "from Device
    /// (default)", "61 % from Game for mGBA". Absent when the row has no
    /// layered value. Surfaces show it beside the value; they never parse it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Whether the value stays on this device (`device`, shown as
    /// "device-only") or may follow the person to other devices (`shared`).
    /// Absent when the row is not such a value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<SurfaceSettingScope>,
}

impl SurfaceSettingItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value: None,
            description: None,
            interaction: None,
            source: None,
            scope: None,
        }
    }
}

/// A titled run of settings items. Korri omits groups it has nothing to say
/// about, so a surface never renders an empty heading.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSettingGroup {
    pub title: String,
    pub items: Vec<SurfaceSettingItem>,
}

#[typeshare(serialized_as = "SurfaceSettingsStatusWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceSettingsStatus {
    Idle,
    Saving { setting_id: String },
    Problem { setting_id: String, message: String },
}

#[typeshare(serialized_as = "SurfaceIdentityDispositionWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceIdentityDisposition {
    Transfer,
    Delete,
}

#[typeshare(serialized_as = "SurfaceIdentityStatusWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceIdentityStatus {
    Idle,
    Working { operation: String },
    BackupReady { encrypted_secret: String },
    Problem { message: String },
    Switched { owner_public_key: String },
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceIdentityManagement {
    pub local_backup_available: bool,
    pub retired_public_keys: Vec<String>,
    pub status: SurfaceIdentityStatus,
}

// ---------------------------------------------------------------------------
// Catalog (cursor)
// ---------------------------------------------------------------------------

/// Opaque position in one catalog query. The surface never parses it; it
/// hands it back in `CatalogFrom.After` or `CatalogFrom.Before`. A cursor
/// stays valid when games are added elsewhere in the list.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Cursor(pub String);

/// The order of a catalog query. `korri` is the order Korri publishes
/// (sections in Korri's order); `title` sorts by title; `mostPlayed` by
/// recorded sessions; `recent` by newest session end.
#[typeshare(serialized_as = "CatalogOrderWire")]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CatalogOrder {
    #[default]
    Korri,
    Title,
    MostPlayed,
    Recent,
}

/// What a catalog window answers.
#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogQuery {
    /// A case-insensitive substring of the title, trimmed. Empty: no filter.
    pub text: String,
    /// Only games whose `SurfaceGame.section` equals this. Absent: every
    /// section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    pub order: CatalogOrder,
}

/// Where a catalog window starts.
#[typeshare(serialized_as = "CatalogFromWire")]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum CatalogFrom {
    /// The first `count` games.
    Start,
    /// The last `count` games.
    End,
    /// The games after the cursor.
    After { cursor: Cursor },
    /// The games before the cursor.
    Before { cursor: Cursor },
    /// A window that holds this game, so focus can stay on it after the query
    /// changes. When the game does not match the query, the window starts at
    /// the start.
    Around { game_id: String },
}

/// The most games one `MoveCatalog` window may hold.
pub const CATALOG_MAX_COUNT: u16 = 200;
/// The window size korrid uses until a surface asks otherwise.
pub const CATALOG_DEFAULT_COUNT: u16 = 50;

/// What Korri currently knows about the things that can be played, as one
/// window of the surface's current query.
#[typeshare(serialized_as = "SurfaceCatalogWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceCatalog {
    Loading,
    Error {
        message: String,
    },
    /// Korri knows no game at all. A query that matches nothing is `Ready`
    /// with `total` 0.
    Empty,
    Ready {
        /// Games matching the current query.
        total: u32,
        /// Consecutive games, in query order.
        window: Vec<SurfaceGame>,
        /// Position of `window[0]` within `total`.
        first_index: u32,
        /// Absent at the start.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<Cursor>,
        /// Absent at the end.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<Cursor>,
        /// The query this window answers.
        query: CatalogQuery,
    },
}

/// The game a surface reports focus on, with that game's actions.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusedGame {
    pub game_id: String,
    pub actions: Vec<SurfaceAction>,
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// One side of a save split.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSaveSide {
    pub action_id: String,
    pub device: String,
    /// UTC epoch milliseconds, by that device's clock. Display only.
    pub ended_at: f64,
    pub duration_seconds: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

/// Where the current launch stands. A cancellable `Busy` state carries its
/// host-authorized actions. `Running` is informational; `Problem` offers
/// recovery.
///
/// - `Browsing`: nothing is happening. The surface shows its normal browsing
///   presentation.
/// - `Busy`: work is under way. `kicker` is a short, calm headline
///   ("Starting…"). `detail` is optional progress detail, never an error code
///   or raw output. `progress` is measured progress when the work has a known
///   position; without it the work has no known end. `steps` names a
///   launch's steps and the one running now, when the work is a launch.
///   `actions` are supplied by the host for this work, such as real
///   cancellation; the surface never infers cancellation from a label.
///   `gameId` names the game this work belongs to, when it belongs to
///   exactly one.
/// - `Confirm`: Korri needs the user's say-so before it does something, such
///   as a large download. The surface renders exactly the host's actions and
///   reports the chosen one through `RunAction`. `declineActionId` is the id
///   of the action that declines; Korri sets it on every `Confirm` that has
///   one, and a surface puts its cursor there first.
/// - `declineActionId` on `Busy`, `ChooseSave` and `Paused`, and on
///   `Confirm`: the id of the action that backs out. A surface's Back runs
///   it, when present, before its own Back handling. Surfaces never infer
///   meaning from action ids. `Busy` without `declineActionId`: Back is
///   swallowed. `Confirm` without `declineActionId`: nothing to decline;
///   Back goes to the surface's own handling.
/// - `ChooseSave`: a save split needs a choice before this game can launch.
///   Side action ids go unchanged to `RunAction`. At least two sides.
/// - `Running`: a launch is live. `gameId` is present when Korri knows which.
/// - `Paused`: this device plays on another device's live session, and that
///   device has paused it. Korri hides the stream until it resumes.
/// - `Ended`: the live session this device played on has ended. `reason` is
///   finished, user-facing copy. Acknowledge it with `Dismiss`.
/// - `Problem`: something did not work, in plain language. `reason` is
///   already user-facing copy. `gameId` and `gameTitle` name the game the
///   failure belongs to, when it belongs to exactly one.
#[typeshare(serialized_as = "SurfaceStatusWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceStatus {
    Browsing,
    Busy {
        kicker: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        progress: Option<SurfaceProgress>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        steps: Option<SurfaceSteps>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actions: Option<Vec<SurfaceAction>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decline_action_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game_id: Option<String>,
    },
    Confirm {
        kicker: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        actions: Vec<SurfaceAction>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decline_action_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game_id: Option<String>,
    },
    ChooseSave {
        game_id: String,
        kicker: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        actions: Vec<SurfaceAction>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decline_action_id: Option<String>,
        sides: Vec<SurfaceSaveSide>,
    },
    Running {
        kicker: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game_id: Option<String>,
    },
    Paused {
        kicker: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        actions: Vec<SurfaceAction>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decline_action_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game_id: Option<String>,
    },
    Ended {
        kicker: String,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game_title: Option<String>,
    },
    Problem {
        kicker: String,
        reason: String,
        can_retry: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game_title: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Gameplay overlay
// ---------------------------------------------------------------------------

/// Presentation-safe gameplay interaction. Integration effects and protected
/// platform instructions deliberately have no representation in this treaty.
#[typeshare(serialized_as = "SurfaceGameplayControlInteractionWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SurfaceGameplayControlInteraction {
    Command,
    Toggle {
        value: bool,
        true_label: String,
        false_label: String,
    },
    Choice {
        value: String,
        options: Vec<SurfaceSettingChoice>,
    },
    Range {
        value: f64,
        min: f64,
        max: f64,
        step: f64,
    },
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceGameplayControl {
    /// Opaque within the current gameplay-overlay model.
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    pub destructive: bool,
    pub dismiss_on_success: bool,
    pub interaction: SurfaceGameplayControlInteraction,
}

/// A plugin- or local-viewer-owned run of controls. Korri omits empty groups.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceGameplayControlGroup {
    pub id: String,
    pub label: String,
    pub controls: Vec<SurfaceGameplayControl>,
}

/// Current local viewer measurements. Missing readings are not zero.
/// Extracted from legacy StreamHealthSample; drops count one sample interval.
#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStreamHealth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_bitrate_kbps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames_dropped: Option<f64>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceGameplayOverlayPresentation {
    /// Phone/device capacity, projected from the local battery producer (0–100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery_percent: Option<f64>,
    /// Current valid measurements of this device's exact stream viewer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_health: Option<SurfaceStreamHealth>,
    pub kind: SurfaceGameplayOverlayKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Overlay-owned controls (Resume, End, Disconnect, Open Korri), before plugin
    /// groups. Hosts publish only the controls this session supports; ids are opaque.
    pub controls: Vec<SurfaceGameplayControl>,
    pub groups: Vec<SurfaceGameplayControlGroup>,
    /// Who plays on this session's stream: person names, or device labels
    /// when a person has none. Absent when nobody does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_players: Option<Vec<String>>,
}

#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceCatalogPresentation {
    pub kind: SurfaceCatalogKind,
}

#[typeshare(serialized_as = "SurfacePresentationWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SurfacePresentation {
    Catalog(SurfaceCatalogPresentation),
    GameplayOverlay(SurfaceGameplayOverlayPresentation),
}

impl SurfacePresentation {
    pub fn catalog() -> Self {
        Self::Catalog(SurfaceCatalogPresentation::default())
    }
}

#[typeshare(serialized_as = "SurfaceGameplayControlValueWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SurfaceGameplayControlValue {
    Toggle { value: bool },
    Choice { value: String },
    Range { value: f64 },
}

// ---------------------------------------------------------------------------
// Runner choice
// ---------------------------------------------------------------------------

/// Installed local runner facts from GameRoutes, not device/location choices.
/// Build identities are exact package paths, never inferred version labels.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceRunnerRoute {
    pub runner_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_id: Option<String>,
    pub system_id: String,
    pub runner_build: String,
    pub program: String,
    pub warnings: Vec<String>,
    pub actions: Vec<SurfaceAction>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceRunnerSaved {
    pub label: String,
    pub runner_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceRunnerChoiceContent {
    pub game_title: String,
    pub message: String,
    pub routes: Vec<SurfaceRunnerRoute>,
    pub saved: Vec<SurfaceRunnerSaved>,
    pub warnings: Vec<String>,
    /// IDs go unchanged to RunAction. The host binds them to this route read.
    pub actions: Vec<SurfaceAction>,
}

#[typeshare(serialized_as = "SurfaceRunnerChoiceWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag")]
pub enum SurfaceRunnerChoice {
    Closed,
    Loading(SurfaceRunnerChoiceContent),
    Ready(SurfaceRunnerChoiceContent),
    Stale(SurfaceRunnerChoiceContent),
    Conflict(SurfaceRunnerChoiceContent),
    Error(SurfaceRunnerChoiceContent),
    Busy(SurfaceRunnerChoiceContent),
    Warnings(SurfaceRunnerChoiceContent),
}

// ---------------------------------------------------------------------------
// Launch settings
// ---------------------------------------------------------------------------

/// The layer a launch settings place belongs to. `id` is opaque; `title` is
/// display text.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceLaunchSettingsLayer {
    pub id: String,
    pub title: String,
}

#[typeshare(serialized_as = "SurfaceLaunchSettingsScopeKindWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceLaunchSettingsScopeKind {
    Any,
    Family,
    Runner,
}

/// Which runners a place applies to. `label` is display text.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceLaunchSettingsScope {
    pub kind: SurfaceLaunchSettingsScopeKind,
    pub label: String,
}

/// One place of a game's launch settings: a layer (Device, a System, the
/// Game, a Release, a Profile) limited to a runner scope (any runner, one
/// runner family, one runner). For each value the most specific place that
/// sets it wins. `id` is opaque and stable across models, so a surface may
/// keep the place it shows. Every row id goes unchanged to `ChangeSetting`,
/// every action id to `RunAction`.
///
/// Places of one layer arrive consecutively, most general layer first, and
/// within a layer any runner first, then each family, then each runner. A
/// surface groups on a change of `layer.id`, and may show a layer's places as
/// one list or one at a time.
///
/// Rows Korri lets the person change carry an interaction. Values set only in
/// files (environment, arguments, working folder, hooks, routines, runner
/// config) are rows without one, with `source` and `scope`. A runner place
/// carries an "Add setting" choice row whose choices are the runner's
/// settings not yet set there; its `selected` is "" (none). Profile controls
/// (pick, new, rename, delete) are ordinary rows: the pick and New profile in
/// the Game place for any runner, Name and Delete in each Profile place for
/// any runner.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceLaunchSettingsPlace {
    pub id: String,
    /// The whole place in Korri's words, such as "Game for mGBA".
    pub title: String,
    pub layer: SurfaceLaunchSettingsLayer,
    pub scope: SurfaceLaunchSettingsScope,
    pub groups: Vec<SurfaceSettingGroup>,
}

/// A person's named variation of a game's launch settings.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceLaunchProfile {
    pub id: String,
    pub title: String,
}

#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceLaunchSettingsContent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_title: Option<String>,
    /// Empty while Loading, and when Korri could not read them.
    pub places: Vec<SurfaceLaunchSettingsPlace>,
    pub profiles: Vec<SurfaceLaunchProfile>,
    /// The profile the next launch uses. Absent: none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_profile_id: Option<String>,
    /// Plain lines about the whole editor, in Korri's words: places Korri
    /// cannot show yet, a runner a place names that is not installed, a
    /// runner whose launch facts could not be read. Empty when there is
    /// nothing to say.
    pub notes: Vec<String>,
    /// Host actions such as Reload, Retry and Close. Ids go unchanged to
    /// RunAction; the host's close action ends with ":cancel".
    pub actions: Vec<SurfaceAction>,
}

/// `Conflict` and `Error` carry user-facing `message` copy beside the content.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceLaunchSettingsProblem {
    pub message: String,
    #[serde(flatten)]
    pub content: SurfaceLaunchSettingsContent,
}

/// The launch settings editor for one game, opened from a game action and
/// closed by the host's close action, as `runnerChoice` is. Unlike the runner
/// chooser, Back reaches the surface while the editor is open, because the
/// editor can hold questions of the surface's own (typing a profile name, a
/// delete confirmation): the surface closes those first, then runs the close
/// action. A write in progress is `settingsStatus` Saving with the row's id.
/// `Conflict` means the values changed elsewhere and must be reloaded;
/// `Error` carries Korri's message and a Retry action. Both carry user-facing
/// `message` copy.
#[typeshare(serialized_as = "SurfaceLaunchSettingsWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag")]
pub enum SurfaceLaunchSettings {
    Closed,
    Loading(SurfaceLaunchSettingsContent),
    Ready(SurfaceLaunchSettingsContent),
    Conflict(SurfaceLaunchSettingsProblem),
    Error(SurfaceLaunchSettingsProblem),
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

/// How this device reaches the network right now. Grounded in legacy
/// `DeviceNetworkState` (`product/platform/device/device-facts.ts`: kind
/// wifi/ethernet, name, strengthPercent) and NetworkManager's primary
/// connection. `Wired` covers any non-Wi-Fi link NetworkManager reports as up.
/// `strengthPercent` is NetworkManager `AccessPoint.Strength`, 0 to 100.
#[typeshare(serialized_as = "SurfaceNetworkConnectionWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceNetworkConnection {
    Offline,
    Wired,
    Wifi {
        ssid: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strength_percent: Option<u32>,
    },
}

/// One Wi-Fi network as NetworkManager's scan reports it. Access points that
/// share an SSID appear once, at their strongest. `id` is opaque: hand it back
/// unchanged to `ConnectWifi`.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceWifiNetwork {
    pub id: String,
    pub ssid: String,
    /// NetworkManager `AccessPoint.Strength`, 0 to 100.
    pub strength_percent: u32,
    /// True when the access point asks for a key (WPA/RSN flags set).
    pub secured: bool,
    /// True when NetworkManager already holds a saved profile for this SSID,
    /// so connecting needs no password.
    pub known: bool,
}

/// The last connection attempt. Korri hands finished, user-facing copy.
/// `passwordRejected`: NetworkManager rejected the key (state reason
/// no-secrets or supplicant failure). The surface may offer to type it again.
#[typeshare(serialized_as = "SurfaceWifiAttemptWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceWifiAttempt {
    Idle,
    Connecting {
        ssid: String,
    },
    Failed {
        ssid: String,
        message: String,
        password_rejected: bool,
    },
}

/// `NoAdapter`: NetworkManager reports no Wi-Fi device on this machine.
/// `Available`: `scanning` while a scan Korri asked for still runs;
/// `networks` strongest first, empty when the last scan found nothing.
#[typeshare(serialized_as = "SurfaceWifiWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceWifi {
    NoAdapter,
    Available {
        scanning: bool,
        networks: Vec<SurfaceWifiNetwork>,
        attempt: SurfaceWifiAttempt,
    },
}

/// Present only when this host can read and change the device's network.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceNetwork {
    pub connection: SurfaceNetworkConnection,
    pub wifi: SurfaceWifi,
}

// ---------------------------------------------------------------------------
// Onboarding
// ---------------------------------------------------------------------------

/// A controller the host can see. Grounded in the native input stream's
/// `device-added` (`deviceId`, `name`, class `gamepad`).
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceController {
    pub id: String,
    pub name: String,
    /// The host has received input from it since onboarding began.
    pub heard: bool,
}

/// The Starter pack this device ships turned off, offered once during setup.
/// `message` is finished, user-facing copy from Korri.
#[typeshare(serialized_as = "SurfaceStarterPackWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag")]
pub enum SurfaceStarterPack {
    Offered {
        actions: Vec<SurfaceAction>,
    },
    Installing,
    Installed,
    Failed {
        message: String,
        actions: Vec<SurfaceAction>,
    },
}

/// Present only while this device has not finished first-boot setup. The
/// surface owns the steps and their order; it ends setup with
/// `FinishOnboarding`, after which Korri omits this field.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceOnboarding {
    pub controllers: Vec<SurfaceController>,
    /// Present while setup can offer the Starter pack. Absent when there is
    /// nothing to offer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starter_pack: Option<SurfaceStarterPack>,
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Getting a game from a provider: a place, such as the PICO-8 BBS, that a
/// provider plugin can search and fetch from. Each surface lists whatever
/// providers Korri publishes; there may be one. Grounded in the legacy store
/// (search, results, detail, Get/Getting/Play, the ungetable guard) and the
/// legacy acquisition protocol (`ProviderClaim`, `ProviderClaimDetails`,
/// `AcquireJobStatus`).
///
/// The host owns every request, the download and its job; the surface owns
/// which of these it shows and when. Each value below is what Korri last
/// learned for the one provider list, the one search and the one open entry
/// it holds. Every action is published by the host and run through
/// `RunAction` unchanged; an action the host no longer publishes does
/// nothing.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStoreProvider {
    /// Opaque. Hand it back unchanged to `SearchStore`.
    pub id: String,
    pub title: String,
}

/// One game a provider offers, in the provider's words. It carries no art:
/// a provider's images are on its own site, which Korri never reaches, so a
/// surface draws its own stand-in.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStoreEntry {
    /// Opaque. Hand it back unchanged to `OpenStoreEntry`.
    pub id: String,
    pub title: String,
    /// What the game runs on, as the provider says it, such as "PICO-8".
    /// Absent when the provider does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
}

/// `Problem`: Korri could not list its providers; `actions` holds its Retry.
/// `Ready`: empty when no provider plugin is enabled on this device.
#[typeshare(serialized_as = "SurfaceStoreProvidersWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag")]
pub enum SurfaceStoreProviders {
    Loading,
    Problem {
        message: String,
        actions: Vec<SurfaceAction>,
    },
    Ready {
        providers: Vec<SurfaceStoreProvider>,
    },
}

/// `Idle`: nothing has been asked yet. `Problem`: the provider did not answer,
/// or answered with something Korri could not read; `message` is
/// user-facing; `actions` holds its Retry. `Ready`: `entries` is empty when
/// the provider found nothing.
#[typeshare(serialized_as = "SurfaceStoreSearchWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceStoreSearch {
    Idle,
    Searching {
        provider_id: String,
        query: String,
    },
    Problem {
        provider_id: String,
        query: String,
        message: String,
        actions: Vec<SurfaceAction>,
    },
    Ready {
        provider_id: String,
        query: String,
        entries: Vec<SurfaceStoreEntry>,
    },
}

/// Where getting the open entry stands. Every string is finished, user-facing
/// copy; a surface renders the actions it is given and infers nothing from a
/// label.
///
/// - `Offer`: Get is on offer (`actions`). `note` says what happened last,
///   such as a cancelled download.
/// - `Unavailable`: this entry cannot be fetched to this device, and why.
///   Nothing was downloaded.
/// - `Confirm`: Korri needs the person's say-so first: the download and its
///   size.
/// - `Working`: under way, in Korri's words. `queued` while it waits its
///   turn, `progress` while it moves, neither while there is no position
///   (finding the file, placing it). `actions` holds Cancel while cancelling
///   is real.
/// - `InLibrary`: the game is in the library. `gameId` is its
///   `SurfaceGame.id` for `LaunchGame`, present once the catalog holds it.
/// - `Problem`: the download did not finish. `actions` holds Retry when one
///   helps.
#[typeshare(serialized_as = "SurfaceStoreGetWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceStoreGet {
    Offer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        actions: Vec<SurfaceAction>,
    },
    Unavailable {
        reason: String,
    },
    Confirm {
        kicker: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        actions: Vec<SurfaceAction>,
    },
    Working {
        kicker: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        actions: Vec<SurfaceAction>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        queued: Option<SurfaceTrue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        progress: Option<SurfaceProgress>,
    },
    InLibrary {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game_id: Option<String>,
    },
    Problem {
        kicker: String,
        reason: String,
        actions: Vec<SurfaceAction>,
    },
}

#[typeshare(serialized_as = "SurfaceStoreDetailsWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag")]
pub enum SurfaceStoreDetails {
    Closed,
    Loading {
        entry: SurfaceStoreEntry,
    },
    Problem {
        entry: SurfaceStoreEntry,
        message: String,
        actions: Vec<SurfaceAction>,
    },
    Ready {
        entry: SurfaceStoreEntry,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        get: SurfaceStoreGet,
    },
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStore {
    pub providers: SurfaceStoreProviders,
    pub search: SurfaceStoreSearch,
    /// The entry last opened with `OpenStoreEntry`. A download keeps going
    /// while another entry is open, and shows again when its entry is.
    pub details: SurfaceStoreDetails,
}

// ---------------------------------------------------------------------------
// Save history and invites
// ---------------------------------------------------------------------------

/// One entry in a game's save history, newest first.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSaveHistoryEntry {
    pub snapshot_id: String,
    /// Display name of the device that made it.
    pub device: String,
    /// Session end, UTC epoch milliseconds, by that device's clock. Display
    /// only.
    pub ended_at: f64,
    pub duration_seconds: f64,
    /// Made from what the game wrote before a crash or stop, not a clean exit.
    pub interrupted: bool,
    /// The snapshot this device continues from.
    pub current: bool,
    /// Host action that restores this entry. Absent on the current entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore: Option<SurfaceAction>,
}

/// Present while history is open. Ready with no entries is an empty history.
/// Actions include the host's close action and go unchanged to RunAction.
#[typeshare(serialized_as = "SurfaceSaveHistoryWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceSaveHistory {
    Loading {
        game_id: String,
        actions: Vec<SurfaceAction>,
    },
    Error {
        game_id: String,
        message: String,
        actions: Vec<SurfaceAction>,
    },
    Ready {
        game_id: String,
        entries: Vec<SurfaceSaveHistoryEntry>,
        actions: Vec<SurfaceAction>,
    },
}

/// Another device of the same person holds a live session that this device
/// can take part in through shared play. `actions` are the routes this device
/// can take (stream play today) and a dismiss. Run them through `RunAction`
/// unchanged; the surface never infers a route from a label.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceInvite {
    /// Opaque. Stable for one live session on one device.
    pub id: String,
    /// The person name, or the device label when the person has none.
    pub person_name: String,
    pub game_title: String,
    /// The label of the device that holds the session.
    pub device_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_art_url: Option<String>,
    pub actions: Vec<SurfaceAction>,
}

/// One display a surface draws on, in logical pixels of the surface's
/// viewport.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceDisplay {
    pub id: String,
    pub main: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub touch: bool,
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Everything Korri publishes to the surface. Replaced wholesale on change.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceModel {
    /// Present only for a surface registered with displays: "all".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub displays: Option<Vec<SurfaceDisplay>>,
    pub presentation: SurfacePresentation,
    pub catalog: SurfaceCatalog,
    pub status: SurfaceStatus,
    /// Absent when save history is not open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_history: Option<SurfaceSaveHistory>,
    /// Absent in hosts that do not offer installed Linux runner selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_choice: Option<SurfaceRunnerChoice>,
    /// Absent in hosts that cannot edit launch settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_settings: Option<SurfaceLaunchSettings>,
    /// Preformatted local time. Absent when the surface should show no clock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_label: Option<String>,
    /// Device-level actions for permissions and session control. May be empty.
    pub actions: Vec<SurfaceAction>,
    /// Device facts and settings, grouped. Empty when Korri can state nothing.
    pub settings: Vec<SurfaceSettingGroup>,
    pub settings_status: SurfaceSettingsStatus,
    /// Present only when this device can manage its person identity locally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_management: Option<SurfaceIdentityManagement>,
    /// Free-form build/identity stamp a surface may display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_label: Option<String>,
    /// Absent in hosts that cannot manage the device's network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<SurfaceNetwork>,
    /// Absent once first-boot setup is finished, and in hosts without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onboarding: Option<SurfaceOnboarding>,
    /// Absent in hosts that cannot get games from providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<SurfaceStore>,
    /// Absent in hosts without shared play. Empty when no invite stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invites: Option<Vec<SurfaceInvite>>,
    /// The game the surface last reported with `FocusGame`, with its actions.
    /// Absent until a `FocusGame` names a game Korri knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_game: Option<FocusedGame>,
}

// ---------------------------------------------------------------------------
// Messages to the surface
// ---------------------------------------------------------------------------

/// A semantic button. The shell, not the surface, decides how each device
/// maps to these, so a surface never learns which device produced one.
#[typeshare(serialized_as = "SurfaceButtonWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceButton {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Options,
    Menu,
    System,
}

/// Something that happened, delivered to the surface's `update`.
///
/// Directional repeat: the first repeat after 400 ms, then every 100 ms.
#[typeshare(serialized_as = "SurfaceMessageWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceMessage {
    /// A button was pressed, or repeats while held.
    Pressed { button: SurfaceButton },
    /// A new model arrived.
    KorriPublished,
    /// A `RememberPreference` completed.
    PreferenceStored { key: String },
    /// The shell sends it to the active surface once a second. `idle_ms` is
    /// the time since the person last pressed a button or touched the
    /// surface (any `Pressed`, and any pointer activity: tap, drag, release),
    /// or since the surface loaded if neither came after; each resets it to 0.
    /// Each surface owns its own idle threshold.
    Tick { idle_ms: u32 },
    /// The surface window's size in logical pixels. Sent once after init,
    /// before the first view, and on every size or scale change.
    Resized { width: f32, height: f32 },
    /// The person touched (or clicked) a target the surface drew (ADR 0055).
    /// `target` is the surface's own focus-target id, reported by its view.
    /// The surface moves focus there and does what Confirm does there.
    /// `at` is the position from 0 to 1 along a range control's track, set
    /// only for range targets. `released` is set on the message sent when the
    /// finger lifts.
    Activated {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        released: Option<bool>,
    },
    /// A finger dragged or flung a row that draws only the items near focus
    /// (ADR 0055). `region` is the surface's own focus-region name; `by` is the
    /// signed number of whole items moved since the last `Dragged`. The
    /// surface moves focus by `by` within the region, clamped at its ends.
    Dragged { region: String, by: i32 },
    /// A finger went down anywhere on the surface (ADR 0055). The shell sends
    /// it once per gesture, before any `Activated` or `Dragged` that gesture
    /// produces. It says only that touch is in use, so the surface can hide
    /// its focus highlight until the next `Pressed`. It names no target and
    /// does not wake an idle surface by itself.
    Touched,
}

// ---------------------------------------------------------------------------
// Requests from the surface
// ---------------------------------------------------------------------------

/// Everything a surface may ask Korri to do. `update` returns these; the shell
/// hands them to korrid. Each is fire-and-forget: the consequence arrives as
/// a new model.
#[typeshare(serialized_as = "SurfaceRequestWire")]
#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceRequest {
    /// Start or resume the game. A location id is required when the
    /// corresponding SurfaceGame publishes launchLocations; otherwise it is
    /// absent.
    LaunchGame {
        game_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        launch_location_id: Option<String>,
    },
    /// Run an action id Korri published, either device-level or setting-local.
    RunAction {
        action_id: String,
    },
    /// Change an editable setting. Korri validates and republishes the result.
    ChangeSetting {
        setting_id: String,
        value: String,
    },
    /// Dismiss the current settings failure without changing its value.
    DismissSettingsProblem,
    ExportIdentityBackup {
        password: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retired_public_key: Option<String>,
    },
    SwitchIdentityFromBackup {
        encrypted_secret: String,
        password: String,
        disposition: SurfaceIdentityDisposition,
        trust_loss_confirmed: bool,
    },
    SwitchIdentityToNip46 {
        bunker_uri: String,
        disposition: SurfaceIdentityDisposition,
        trust_loss_confirmed: bool,
    },
    DeleteRetiredIdentity {
        public_key: String,
        backup_confirmed: bool,
    },
    DismissIdentityStatus,
    RunGameAction {
        game_id: String,
        action_id: String,
    },
    /// Invoke a control from the current gameplay-overlay model. Korri binds
    /// this opaque request to its current launch identity.
    InvokeGameplayControl {
        control_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<SurfaceGameplayControlValue>,
    },
    /// Ask to resume from the gameplay overlay. Korri binds this to the exact
    /// live launch; the surface never closes or thaws a session itself.
    DismissGameplayOverlay,
    /// Try the failed thing again. Only meaningful while `Problem.canRetry`.
    Retry,
    /// Acknowledge a `Problem` and return to browsing.
    Dismiss,
    /// Ask Korri to re-read everything (used by error recovery).
    Reload,
    /// Ask for a fresh Wi-Fi scan. Results arrive in `model.network`.
    ScanWifi,
    /// Join a listed network. `password` is required for a secured network
    /// that is not `known`, and is passed straight to NetworkManager: Korri
    /// never stores, logs, or republishes it. For a `known` network, a
    /// `password` replaces the key NetworkManager saved for it; without one,
    /// the saved key is used.
    ConnectWifi {
        network_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
    },
    /// Join a network that does not broadcast its name. With a password Korri
    /// asks for WPA-PSK; without one it joins an open network.
    ConnectHiddenWifi {
        ssid: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
    },
    /// Record that first-boot setup is finished on this device.
    FinishOnboarding,
    /// Read the providers again. The list arrives in `model.store.providers`.
    OpenStore,
    /// Search one provider. Results arrive in `model.store.search`; a newer
    /// search replaces an older one.
    SearchStore {
        provider_id: String,
        query: String,
    },
    /// Read one entry of the current results. It arrives in
    /// `model.store.details`, with where getting it stands.
    OpenStoreEntry {
        entry_id: String,
    },
    /// The game that has focus now, or none. Korri publishes that game's
    /// actions as `model.focusedGame`.
    FocusGame {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game_id: Option<String>,
    },
    /// Store one declared preference of this surface. Korri answers with a
    /// `PreferenceStored` message.
    RememberPreference {
        key: String,
        value: String,
    },
    /// Ask for another catalog window. `count` is capped at
    /// `CATALOG_MAX_COUNT`.
    MoveCatalog {
        query: CatalogQuery,
        from: CatalogFrom,
        count: u16,
    },
}

impl SurfaceRequest {
    /// The variant name, safe to log: never a password, secret or value.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::LaunchGame { .. } => "LaunchGame",
            Self::RunAction { .. } => "RunAction",
            Self::ChangeSetting { .. } => "ChangeSetting",
            Self::DismissSettingsProblem => "DismissSettingsProblem",
            Self::ExportIdentityBackup { .. } => "ExportIdentityBackup",
            Self::SwitchIdentityFromBackup { .. } => "SwitchIdentityFromBackup",
            Self::SwitchIdentityToNip46 { .. } => "SwitchIdentityToNip46",
            Self::DeleteRetiredIdentity { .. } => "DeleteRetiredIdentity",
            Self::DismissIdentityStatus => "DismissIdentityStatus",
            Self::RunGameAction { .. } => "RunGameAction",
            Self::InvokeGameplayControl { .. } => "InvokeGameplayControl",
            Self::DismissGameplayOverlay => "DismissGameplayOverlay",
            Self::Retry => "Retry",
            Self::Dismiss => "Dismiss",
            Self::Reload => "Reload",
            Self::ScanWifi => "ScanWifi",
            Self::ConnectWifi { .. } => "ConnectWifi",
            Self::ConnectHiddenWifi { .. } => "ConnectHiddenWifi",
            Self::FinishOnboarding => "FinishOnboarding",
            Self::OpenStore => "OpenStore",
            Self::SearchStore { .. } => "SearchStore",
            Self::OpenStoreEntry { .. } => "OpenStoreEntry",
            Self::FocusGame { .. } => "FocusGame",
            Self::RememberPreference { .. } => "RememberPreference",
            Self::MoveCatalog { .. } => "MoveCatalog",
        }
    }
}

/// Requests carry passwords and identity secrets. Debug prints only the tag.
impl std::fmt::Debug for SurfaceRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "SurfaceRequest::{}", self.tag())
    }
}

// ---------------------------------------------------------------------------
// Preferences
// ---------------------------------------------------------------------------

/// Whether a preference follows the person (`shared`) or stays on the device
/// (`device`). The mark belongs to the kind (ADR 0018).
#[typeshare(serialized_as = "SurfacePreferenceMarkWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfacePreferenceMark {
    Shared,
    Device,
}

/// One preference kind a package declares next to its logic.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfacePreferenceDeclaration {
    pub mark: SurfacePreferenceMark,
    pub initial: String,
}

/// Which displays a package draws on: `globalThis.surface.displays` in its
/// `logic.js`, which the shell hands korrid with `app.surface.declare`.
/// Absent means `main`. `main`: the main display only. `all`: every display;
/// korrid places the window across them and publishes `SurfaceModel.displays`.
/// Grounding: the portal's surface registry (`clients/portal/src/surface/
/// surface-registry.tsx`): Pico `all`, Zone and Boxbuster `main`.
#[typeshare(serialized_as = "SurfacePackageDisplaysWire")]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfacePackageDisplays {
    #[default]
    Main,
    All,
}

/// A package's `preferences` declaration: key to declaration.
#[typeshare]
pub type SurfacePreferenceDeclarations = HashMap<String, SurfacePreferenceDeclaration>;

/// Stored preference values of one surface: key to value.
#[typeshare]
pub type SurfacePreferenceValues = HashMap<String, String>;

/// At most this many preference keys per surface.
pub const PREFERENCE_MAX_KEYS: usize = 16;
/// A key is `[a-z0-9.-]{1,PREFERENCE_MAX_KEY_LENGTH}`.
pub const PREFERENCE_MAX_KEY_LENGTH: usize = 32;
/// A value is UTF-8 of at most this many bytes.
pub const PREFERENCE_MAX_VALUE_BYTES: usize = 256;

/// Why a preference declaration or value is refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreferenceRefusal {
    TooManyKeys,
    InvalidKey(String),
    ValueTooLong(String),
    Undeclared(String),
}

impl std::fmt::Display for PreferenceRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyKeys => write!(
                formatter,
                "a surface declares at most {PREFERENCE_MAX_KEYS} preferences"
            ),
            Self::InvalidKey(key) => write!(
                formatter,
                "preference key {key:?} is not [a-z0-9.-]{{1,{PREFERENCE_MAX_KEY_LENGTH}}}"
            ),
            Self::ValueTooLong(key) => write!(
                formatter,
                "preference {key:?} is longer than {PREFERENCE_MAX_VALUE_BYTES} bytes"
            ),
            Self::Undeclared(key) => write!(formatter, "preference {key:?} is not declared"),
        }
    }
}

/// `[a-z0-9.-]{1,32}`.
pub fn valid_preference_key(key: &str) -> bool {
    (1..=PREFERENCE_MAX_KEY_LENGTH).contains(&key.len())
        && key.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'-'
        })
}

/// Check a package's declaration: key count, key spelling and initial sizes.
pub fn check_preference_declarations(
    declarations: &SurfacePreferenceDeclarations,
) -> Result<(), PreferenceRefusal> {
    if declarations.len() > PREFERENCE_MAX_KEYS {
        return Err(PreferenceRefusal::TooManyKeys);
    }
    let mut keys: Vec<_> = declarations.keys().collect();
    keys.sort();
    for key in keys {
        if !valid_preference_key(key) {
            return Err(PreferenceRefusal::InvalidKey(key.clone()));
        }
        if declarations[key].initial.len() > PREFERENCE_MAX_VALUE_BYTES {
            return Err(PreferenceRefusal::ValueTooLong(key.clone()));
        }
    }
    Ok(())
}

/// Check one value a surface asks to store against its declaration.
pub fn check_preference_value(
    declarations: &SurfacePreferenceDeclarations,
    key: &str,
    value: &str,
) -> Result<SurfacePreferenceMark, PreferenceRefusal> {
    if !valid_preference_key(key) {
        return Err(PreferenceRefusal::InvalidKey(key.to_owned()));
    }
    let declaration = declarations
        .get(key)
        .ok_or_else(|| PreferenceRefusal::Undeclared(key.to_owned()))?;
    if value.len() > PREFERENCE_MAX_VALUE_BYTES {
        return Err(PreferenceRefusal::ValueTooLong(key.to_owned()));
    }
    Ok(declaration.mark)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_types_use_the_agreed_json() {
        assert_eq!(
            serde_json::to_value(SurfaceMessage::Pressed {
                button: SurfaceButton::Up
            })
            .unwrap(),
            json!({"_tag": "Pressed", "button": "up"})
        );
        assert_eq!(
            serde_json::to_value(SurfaceMessage::Activated {
                target: "row:play".into(),
                at: None,
                released: None
            })
            .unwrap(),
            json!({"_tag": "Activated", "target": "row:play"})
        );
        assert_eq!(
            serde_json::to_value(SurfaceMessage::Dragged {
                region: "shelf".into(),
                by: -2
            })
            .unwrap(),
            json!({"_tag": "Dragged", "region": "shelf", "by": -2})
        );
        assert_eq!(
            serde_json::to_value(SurfaceMessage::Touched).unwrap(),
            json!({"_tag": "Touched"})
        );
        let slid: SurfaceMessage = serde_json::from_value(
            json!({"_tag": "Activated", "target": "control:volume", "at": 0.5, "released": true}),
        )
        .unwrap();
        assert_eq!(
            slid,
            SurfaceMessage::Activated {
                target: "control:volume".into(),
                at: Some(0.5),
                released: Some(true)
            }
        );
        assert_eq!(
            serde_json::to_value(&slid).unwrap(),
            json!({"_tag": "Activated", "target": "control:volume", "at": 0.5, "released": true})
        );
        assert_eq!(
            serde_json::to_value(SurfaceMessage::Tick { idle_ms: 31000 }).unwrap(),
            json!({"_tag": "Tick", "idleMs": 31000})
        );
        let resized: SurfaceMessage =
            serde_json::from_value(json!({"_tag": "Resized", "width": 640, "height": 480}))
                .unwrap();
        assert_eq!(
            resized,
            SurfaceMessage::Resized {
                width: 640.0,
                height: 480.0
            }
        );
        assert_eq!(
            serde_json::to_value(&resized).unwrap(),
            json!({"_tag": "Resized", "width": 640.0, "height": 480.0})
        );
        assert_eq!(
            serde_json::to_value(SurfaceStatus::Busy {
                kicker: "Checking saves…".into(),
                detail: None,
                progress: None,
                steps: None,
                actions: Some(vec![SurfaceAction::new("saves:cancel", "Cancel", true)]),
                decline_action_id: Some("saves:cancel".into()),
                game_id: None,
            })
            .unwrap(),
            json!({"_tag": "Busy", "kicker": "Checking saves…",
                "actions": [{"id": "saves:cancel", "label": "Cancel", "enabled": true}],
                "declineActionId": "saves:cancel"})
        );
        assert_eq!(
            serde_json::to_value(SurfaceStatus::Busy {
                kicker: "Starting Anguna…".into(),
                detail: None,
                progress: None,
                steps: Some(SurfaceSteps {
                    labels: vec!["Checking saves".into(), "Starting RetroArch".into()],
                    current: 1,
                }),
                actions: None,
                decline_action_id: None,
                game_id: None,
            })
            .unwrap(),
            json!({"_tag": "Busy", "kicker": "Starting Anguna…",
                "steps": {"labels": ["Checking saves", "Starting RetroArch"], "current": 1}})
        );
        let request: SurfaceRequest = serde_json::from_value(json!({
            "_tag": "MoveCatalog",
            "query": {"text": "", "section": null, "order": "korri"},
            "from": {"_tag": "Around", "gameId": "x"},
            "count": 50
        }))
        .unwrap();
        assert_eq!(
            request,
            SurfaceRequest::MoveCatalog {
                query: CatalogQuery::default(),
                from: CatalogFrom::Around {
                    game_id: "x".into()
                },
                count: 50
            }
        );
        assert_eq!(
            serde_json::to_value(CatalogOrder::MostPlayed).unwrap(),
            json!("mostPlayed")
        );
        assert_eq!(
            serde_json::to_value(CatalogFrom::After {
                cursor: Cursor("c".into())
            })
            .unwrap(),
            json!({"_tag": "After", "cursor": "c"})
        );
        assert_eq!(
            serde_json::to_value(SurfaceRequest::FocusGame { game_id: None }).unwrap(),
            json!({"_tag": "FocusGame"})
        );
    }

    #[test]
    fn ported_types_keep_their_json() {
        let status = SurfaceStatus::Problem {
            kicker: "k".into(),
            reason: "r".into(),
            can_retry: false,
            game_id: Some("g".into()),
            game_title: None,
        };
        assert_eq!(
            serde_json::to_value(&status).unwrap(),
            json!({"_tag": "Problem", "kicker": "k", "reason": "r", "canRetry": false, "gameId": "g"})
        );
        assert_eq!(
            serde_json::to_value(SurfacePresentation::catalog()).unwrap(),
            json!({"kind": "catalog"})
        );
        let interaction = SurfaceSettingInteraction::SensitiveText {
            placeholder: None,
            max_length: Some(4),
            clear_label: None,
        };
        assert_eq!(
            serde_json::to_value(&interaction).unwrap(),
            json!({"kind": "sensitiveText", "maxLength": 4})
        );
        let download = SurfaceGameDownload {
            queued: Some(SurfaceTrue),
            progress: None,
            bytes: Some(10.0),
        };
        assert_eq!(
            serde_json::to_value(&download).unwrap(),
            json!({"queued": true, "bytes": 10.0})
        );
        let settings = SurfaceLaunchSettings::Error(SurfaceLaunchSettingsProblem {
            message: "m".into(),
            content: SurfaceLaunchSettingsContent::default(),
        });
        assert_eq!(
            serde_json::to_value(&settings).unwrap(),
            json!({"_tag": "Error", "message": "m", "places": [], "profiles": [], "notes": [], "actions": []})
        );
        let choice = SurfaceRunnerChoice::Closed;
        assert_eq!(
            serde_json::to_value(&choice).unwrap(),
            json!({"_tag": "Closed"})
        );
        let overlay: SurfacePresentation = serde_json::from_value(
            json!({"kind": "gameplay-overlay", "controls": [], "groups": []}),
        )
        .unwrap();
        assert!(matches!(overlay, SurfacePresentation::GameplayOverlay(_)));
        assert_eq!(
            serde_json::to_value(SurfaceGameplayControlValue::Range { value: 1.0 }).unwrap(),
            json!({"kind": "range", "value": 1.0})
        );
    }

    #[test]
    fn request_debug_never_prints_secrets() {
        let request = SurfaceRequest::ConnectWifi {
            network_id: "n".into(),
            password: Some("hunter2".into()),
        };
        assert_eq!(format!("{request:?}"), "SurfaceRequest::ConnectWifi");
    }

    #[test]
    fn preference_rules() {
        assert!(valid_preference_key("pico.font"));
        assert!(valid_preference_key(&"a".repeat(32)));
        assert!(!valid_preference_key(&"a".repeat(33)));
        assert!(!valid_preference_key(""));
        assert!(!valid_preference_key("Zone.look"));
        assert!(!valid_preference_key("zone_look"));
        let mut declarations = SurfacePreferenceDeclarations::new();
        declarations.insert(
            "zone.look".into(),
            SurfacePreferenceDeclaration {
                mark: SurfacePreferenceMark::Shared,
                initial: "modern".into(),
            },
        );
        assert_eq!(check_preference_declarations(&declarations), Ok(()));
        assert_eq!(
            check_preference_value(&declarations, "zone.look", "retro"),
            Ok(SurfacePreferenceMark::Shared)
        );
        assert_eq!(
            check_preference_value(&declarations, "zone.look", &"x".repeat(257)),
            Err(PreferenceRefusal::ValueTooLong("zone.look".into()))
        );
        assert_eq!(
            check_preference_value(&declarations, "zone.other", "x"),
            Err(PreferenceRefusal::Undeclared("zone.other".into()))
        );
        for index in 0..16 {
            declarations.insert(
                format!("k{index}"),
                SurfacePreferenceDeclaration {
                    mark: SurfacePreferenceMark::Device,
                    initial: String::new(),
                },
            );
        }
        assert_eq!(
            check_preference_declarations(&declarations),
            Err(PreferenceRefusal::TooManyKeys)
        );
    }
}
