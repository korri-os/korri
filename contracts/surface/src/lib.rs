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
    /// Opaque: hand it back unchanged as an image, `{ $image: coverArtUrl }`.
    /// The host loads it later, so the image is empty until it arrives, and
    /// stays empty if it cannot be loaded.
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
    /// The system this game runs on, as Korri names it, such as "gba". It is
    /// the value `CatalogQuery.systemId` and `SurfaceSystem.systemId` use.
    /// Absent for a game on another device, and for a game with no system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_id: Option<String>,
    /// The system's name in words, such as "Game Boy Advance": the system's
    /// title, else its id. Absent together with `systemId`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Every image Korri stores for this game, from every art plugin, each
    /// with its measured size (ADR 0091). Absent when Korri has none. The
    /// list has no order a surface may rely on: a surface picks by `kind`
    /// and by the shape and size it reads from `width` and `height`.
    /// `coverArtUrl` stays the square cover.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Vec<SurfaceMedia>>,
    /// Whether this device can install the game (ADR 0092): present only on
    /// a game that only other devices hold, as a file release. Play here is
    /// `InstallGame` with `play`; Korri never picks it for the person. While
    /// the install is in the line, `download` says where it stands, as for
    /// every download. Absent once this device holds the game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_here: Option<SurfaceInstallHere>,
}

/// Why this device cannot install a game now (ADR 0092). The surface words
/// it. `noRunner`: no runner here plays the game's system, and no plugin
/// offers one. `notEnoughSpace`: the release does not fit in the free
/// storage. `noSource`: no device that holds the release answered.
/// `notAllowed`: the devices that hold it do not serve installs to this
/// device (only the owner's and family members' devices may install, ADR
/// 0077).
#[typeshare(serialized_as = "SurfaceInstallBlockWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceInstallBlock {
    NoRunner,
    NotEnoughSpace,
    NoSource,
    NotAllowed,
}

/// Whether this device can install a game that other devices hold. Korri
/// learns the release's size from those devices only when the person asks
/// (`InstallGame`), never from the catalog, so free space and a reachable
/// source are known only after that.
///
/// - `Offered`: this device has a runner for the game. `InstallGame` asks
///   the devices that hold it for its size, checks the free space, and
///   installs.
/// - `NeedsPlugin`: no runner here, but the plugin `pluginTitle` offers
///   one. `InstallGame` first asks to install that plugin, with its
///   permission review (a `Confirm` status, as for netplay), then goes on
///   as for `Offered`.
/// - `Checking`: Korri is asking the devices that hold it for its size.
/// - `Installing`: the install is in the line: `installs` holds the entry
///   whose id is `installId`.
/// - `Unavailable`: not possible now, and why. `bytes` is the release's
///   size when Korri learned it. `InstallGame` checks again.
#[typeshare(serialized_as = "SurfaceInstallHereWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceInstallHere {
    Offered,
    NeedsPlugin {
        plugin_title: String,
    },
    Checking,
    Installing {
        install_id: String,
    },
    Unavailable {
        reason: SurfaceInstallBlock,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bytes: Option<f64>,
    },
}

/// Where one install stands (ADR 0092). One install runs at a time.
///
/// - `Waiting`: in the line, behind the one that runs. `bytes` is the size
///   when Korri knows it.
/// - `Installing`: the one that runs. `progress.total` is the size; absent
///   while the size is unknown (unpacking). `secondsLeft` is Korri's
///   prediction of the time left; absent until Korri has measured a speed.
/// - `Failed`: it stopped, and `reason` says why in user-facing words. Its
///   verified pieces may stay so that a new try resumes; `CancelInstall`
///   throws it away.
#[typeshare(serialized_as = "SurfaceInstallStateWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceInstallState {
    Waiting {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bytes: Option<f64>,
    },
    Installing {
        progress: SurfaceProgress,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seconds_left: Option<f64>,
    },
    Failed {
        reason: String,
    },
}

/// One entry of the list of installs (`SurfaceModel.installs`).
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceInstall {
    /// Opaque. Hand it back unchanged to `MoveInstall`, `CancelInstall` and
    /// `AnswerStreamMeanwhile`.
    pub id: String,
    pub title: String,
    /// The `SurfaceGame.id` of the game, when the catalog holds it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
    pub state: SurfaceInstallState,
    /// Play here started it: Korri starts the game when it is installed,
    /// unless the person started something else meanwhile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_when_ready: Option<SurfaceTrue>,
    /// Korri asks whether to stream the game meanwhile, because the
    /// predicted time is longer than 2 minutes. The install goes on either
    /// way. Answer with `AnswerStreamMeanwhile`; until then the question
    /// stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_meanwhile: Option<SurfaceTrue>,
}

/// What a stored image of a game is, in three broad kinds (ADR 0091).
/// `logo` is the game's title as an image with a transparent background.
/// `picture` is any other image of the game: box art, a grid, a wide hero
/// or a background; its shape is in its measured size, not in its kind.
/// `icon` is a small square mark of the game.
#[typeshare(serialized_as = "SurfaceMediaKindWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceMediaKind {
    Logo,
    Picture,
    Icon,
}

impl SurfaceMediaKind {
    pub const ALL: [Self; 3] = [Self::Logo, Self::Picture, Self::Icon];

    /// The name of this kind on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Logo => "logo",
            Self::Picture => "picture",
            Self::Icon => "icon",
        }
    }
}

/// One stored image of a game. A surface derives its ratio from `width`
/// and `height`; no ratio is sent.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceMedia {
    pub kind: SurfaceMediaKind,
    /// Opaque, as `coverArtUrl`: hand it back unchanged as an image,
    /// `{ $image: url }`. The host loads it later, so the image is empty
    /// until it arrives, and stays empty if it cannot be loaded.
    pub url: String,
    /// The image's own width in pixels, measured by Korri, so a surface can
    /// lay it out before it arrives.
    pub width: u32,
    /// The image's own height in pixels, measured by Korri.
    pub height: u32,
    /// One colour taken from this image, as lowercase `#rrggbb`. Absent when
    /// the image has no clear colour. A surface that tints with it must have
    /// a fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
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
    Problem { message: String },
    Switched { owner_public_key: String },
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceIdentityManagement {
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
    /// Only games whose `SurfaceGame.systemId` equals this. Absent: every
    /// system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_id: Option<String>,
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

/// One system that games of the whole list run on, with how many do.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSystem {
    /// As `SurfaceGame.systemId`. Hand it back in `CatalogQuery.systemId`.
    pub system_id: String,
    /// The system's name in words, as `SurfaceGame.system`.
    pub system: String,
    /// Games of the whole list that run on this system, whatever the
    /// surface's current query is.
    pub total: u32,
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

/// A person's named variation of a game's launch settings.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceLaunchProfile {
    pub id: String,
    pub title: String,
}

/// One row the person changed and has not kept yet (ADR 0066). `id` is the
/// row's id. `was` is the earlier value as display text, such as "4x".
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceLaunchSettingsChange {
    pub id: String,
    pub was: String,
}

/// One game's launch settings as one list (ADR 0066). Each setting appears
/// once, with the value the next launch would use, whether this device runs
/// the game or another device streams it here. A changed row waits as a
/// change. Korri writes the changes to the `keep` place when the editor
/// closes; changes kept for this launch go with that launch and end with it.
///
/// Values set only in files (environment, arguments, working folder, hooks,
/// routines, runner config) are not rows. Profile controls (pick, new, name,
/// delete) are ordinary rows of the last group.
#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceLaunchSettingsContent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_title: Option<String>,
    /// Where the game runs, in Korri's words: "Runs here", "Runs on aka".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs_on: Option<String>,
    /// The list, grouped by setting section. A row Korri cannot change now
    /// (its device cannot be reached, or the runner cannot honour the
    /// setting) has no interaction, and its `description` says why. A row's
    /// `source` says which device keeps the value, when the value describes
    /// one device. Every row id goes unchanged to `ChangeSetting`, every
    /// action id to `RunAction`. Empty while Loading, and when Korri has
    /// nothing to show.
    pub groups: Vec<SurfaceSettingGroup>,
    /// The rows changed and not kept yet. A row is in the list at most once.
    /// Setting a row back to its earlier value takes it out.
    pub changes: Vec<SurfaceLaunchSettingsChange>,
    /// Where the changes are kept. Present only while `changes` is not
    /// empty. A choice row: this launch, this game, every game, then the
    /// other places. Its id goes unchanged to `ChangeSetting`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<SurfaceSettingItem>,
    pub profiles: Vec<SurfaceLaunchProfile>,
    /// The profile the next launch uses. Absent: none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_profile_id: Option<String>,
    /// Plain lines about the whole editor, in Korri's words: settings Korri
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
/// action. A write in progress is `settingsStatus` Saving with the row's id;
/// while the changes are kept, that id is the `keep` row's.
/// `Conflict` means the values changed elsewhere and must be reloaded;
/// `Error` carries Korri's message and a Retry action. A device that runs a
/// streamed game and cannot be reached is an `Error`: its rows stay in the
/// content without an interaction, and the playing device's own rows still
/// work. Both carry user-facing `message` copy.
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

/// This device's battery. Grounded in legacy `DeviceBatteryReady`
/// (`product/platform/device/device-facts.ts`: percent, charging) with
/// `percent` made optional (ADR 0094). `charging` is the power_supply
/// `status` "Charging". `percent` (0 to 100) is present only when the
/// device declares its capacity events verified; korrid never publishes a
/// percent that can be stale.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceBattery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    pub charging: bool,
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

/// One window of a surface that declared panes (ADR 0086), and the panes it
/// holds, in the declared order. Window 0 is the shell's `Surface` window;
/// each further window is another instance of `Surface` that the shell maps.
/// It carries no display facts: korrid placed it, and its size arrives in
/// `Resized` with its index. Grounding: the shell's window roles
/// (`clients/shell/src/shell.rs`) and a game placement's ordered outputs,
/// main first (`services/korrid/src/host/display_placement.rs`).
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceWindow {
    pub panes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Account sign-in (ADR 0083, ADR 0089)
// ---------------------------------------------------------------------------

/// A QR code for the surface to draw. `modules` holds `size` rows of `size`
/// characters each, row by row from the top, `1` for a dark module and `0`
/// for a light one, with no quiet zone. Korri encodes every code at error
/// correction level H, so a logo may cover the middle. The surface draws it
/// at any scale, with a light border of at least four modules.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceQrCode {
    pub size: u32,
    pub modules: String,
}

/// One plugin's link between this device and the person's account at an
/// outside service (CONTEXT.md, "Account sign-in"). Every string is
/// finished, user-facing copy.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceAccountSignIn {
    /// Opaque. Stable for one plugin on this device.
    pub id: String,
    pub title: String,
    /// The service's mark, kind `logo`, served by Korri. Absent when the
    /// plugin ships none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<SurfaceMedia>,
    /// Finished copy for an optional setup step. Absent: setup does not
    /// offer this sign-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offer: Option<String>,
    pub state: SurfaceAccountState,
}

/// One account sign-in's state. Every action runs through `RunAction`
/// unchanged; a surface infers nothing from a label.
///
/// - `SignedOut`: no account is linked; `actions` holds Sign in. `kicker`
///   is present when a sign-in ran out and the service needs a new one.
/// - `Code`: the person approves `qr` on a phone. `forGame` is the title of
///   the game whose launch asked for the sign-in, absent when the person
///   asked from setup or settings. `actions` holds Cancel.
/// - `Working`: approved, or starting; Korri is signing in. `actions` may
///   hold Cancel.
/// - `SignedIn`: `accountName` is the service's account name. `actions`
///   holds Sign out.
/// - `Problem`: sign-in did not finish; `actions` holds Retry when one helps.
#[typeshare(serialized_as = "SurfaceAccountStateWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum SurfaceAccountState {
    SignedOut {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kicker: Option<String>,
        actions: Vec<SurfaceAction>,
    },
    Code {
        qr: SurfaceQrCode,
        kicker: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        for_game: Option<String>,
        actions: Vec<SurfaceAction>,
    },
    Working {
        kicker: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        for_game: Option<String>,
        actions: Vec<SurfaceAction>,
    },
    SignedIn {
        account_name: String,
        actions: Vec<SurfaceAction>,
    },
    Problem {
        kicker: String,
        reason: String,
        actions: Vec<SurfaceAction>,
    },
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Everything Korri publishes to the surface. Replaced wholesale on change.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceModel {
    /// Present only for a package that declared panes: which panes each of
    /// its windows holds (ADR 0086).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<Vec<SurfaceWindow>>,
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
    /// Absent when the device has no battery or korrid cannot read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery: Option<SurfaceBattery>,
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
    /// The systems of the whole game list with their game counts, in title
    /// order. A game without `systemId` is in none of them. Absent while the
    /// catalog loads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub systems: Option<Vec<SurfaceSystem>>,
    /// When Korri built this model, as UTC epoch milliseconds. A surface that
    /// shows how long ago something happened compares against this; it is not
    /// a live clock. Absent when Korri has no trustworthy time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<f64>,
    /// The account sign-ins of this device's plugins, for the current
    /// person, in a stable order. Empty when no plugin has one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub account_sign_ins: Vec<SurfaceAccountSignIn>,
    /// The list of installs on this device (ADR 0092), in the line's order:
    /// the one that runs (`Installing`) first, then those that wait, then
    /// those that failed. Absent in hosts without installs. Empty when
    /// nothing installs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installs: Option<Vec<SurfaceInstall>>,
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
    /// before the first view, and on every size or scale change. `window`:
    /// the index in `SurfaceModel.windows` of a further window (ADR 0086);
    /// absent for window 0.
    Resized {
        width: f32,
        height: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window: Option<u32>,
    },
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
    /// A finger dragged or flung a row that draws only some of its items
    /// (ADR 0055). `region` is the surface's own focus-region name; `by` is the
    /// signed number of whole items moved since the last `Dragged`. The
    /// surface moves that row's scroll position by `by`, clamped at its ends.
    /// Focus does not move.
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
    SwitchIdentityToNip46 {
        bunker_uri: String,
        disposition: SurfaceIdentityDisposition,
        trust_loss_confirmed: bool,
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
    /// Install a game that other devices hold, while its `installHere` is
    /// `Offered`, `NeedsPlugin` or `Unavailable` (ADR 0092). With `play`
    /// (play here) it goes to the front of the line and the game starts when
    /// it is installed; without, it goes to the back. With `play` on a game
    /// that is `Installing`, the game starts when it is installed.
    InstallGame {
        game_id: String,
        play: bool,
    },
    /// Move an install to `index` in `installs`. The install that runs goes
    /// on running, so an index at or before it means next.
    MoveInstall {
        install_id: String,
        index: u32,
    },
    /// Cancel an install that waits or runs, or throw away one that failed.
    CancelInstall {
        install_id: String,
    },
    /// The answer to `SurfaceInstall.streamMeanwhile`. With `stream`, Korri
    /// starts stream play of the game now and the install goes on in the
    /// background; the game then does not start when it is installed.
    /// `launchLocationId` is required with `stream` when the game publishes
    /// `launchLocations`, as for `LaunchGame`. Without `stream`, the person
    /// waits, and the game starts when it is installed.
    AnswerStreamMeanwhile {
        install_id: String,
        stream: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        launch_location_id: Option<String>,
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
            Self::SwitchIdentityToNip46 { .. } => "SwitchIdentityToNip46",
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
            Self::InstallGame { .. } => "InstallGame",
            Self::MoveInstall { .. } => "MoveInstall",
            Self::CancelInstall { .. } => "CancelInstall",
            Self::AnswerStreamMeanwhile { .. } => "AnswerStreamMeanwhile",
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

/// A package's panes (ADR 0086): `globalThis.surface.panes` and
/// `globalThis.surface.arrangement` in its `logic.js`, which the shell hands
/// korrid with `app.surface.declare`. The names and their meaning are a
/// plugin system's own `panes` and `arrangement` (ADR 0039,
/// `services/korrid/src/plugin.rs`): the ordered pane ids, the first for the
/// main display, and how they relate. A package without them has one pane.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfacePackagePanes {
    pub panes: Vec<String>,
    pub arrangement: String,
}

/// The one arrangement korrid places, for games and surfaces alike.
pub const PANES_UPPER_ABOVE_LOWER: &str = "upper-above-lower";

impl SurfacePackagePanes {
    /// Two distinct pane ids, `[a-z][a-z0-9-]{0,31}`, upper above lower:
    /// what korrid's placement supports (`display_placement::choose`).
    pub fn check(&self) -> Result<(), String> {
        let id = |pane: &str| {
            let mut bytes = pane.bytes();
            pane.len() <= 32
                && bytes.next().is_some_and(|b| b.is_ascii_lowercase())
                && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        };
        if self.panes.len() != 2 || self.panes[0] == self.panes[1] {
            return Err("panes must name exactly two different panes".into());
        }
        if !self.panes.iter().all(|pane| id(pane)) {
            return Err("a pane id is [a-z][a-z0-9-]{0,31}".into());
        }
        if self.arrangement != PANES_UPPER_ABOVE_LOWER {
            return Err(format!("arrangement must be \"{PANES_UPPER_ABOVE_LOWER}\""));
        }
        Ok(())
    }
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
                height: 480.0,
                window: None,
            }
        );
        assert_eq!(
            serde_json::to_value(&resized).unwrap(),
            json!({"_tag": "Resized", "width": 640.0, "height": 480.0})
        );
        assert_eq!(
            serde_json::to_value(SurfaceMessage::Resized {
                width: 1240.0,
                height: 1080.0,
                window: Some(1),
            })
            .unwrap(),
            json!({"_tag": "Resized", "width": 1240.0, "height": 1080.0, "window": 1})
        );
        let panes = |panes: &[&str], arrangement: &str| SurfacePackagePanes {
            panes: panes.iter().map(|p| p.to_string()).collect(),
            arrangement: arrangement.into(),
        };
        assert!(panes(&["stage", "floor"], "upper-above-lower")
            .check()
            .is_ok());
        assert!(panes(&["stage"], "upper-above-lower").check().is_err());
        assert!(panes(&["stage", "stage"], "upper-above-lower")
            .check()
            .is_err());
        assert!(panes(&["Stage", "floor"], "upper-above-lower")
            .check()
            .is_err());
        assert!(panes(&["stage", "floor"], "side-by-side").check().is_err());
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
    fn systems_and_media_use_the_agreed_json() {
        let media = SurfaceMedia {
            kind: SurfaceMediaKind::Picture,
            url: "/game-assets/aa.png".into(),
            width: 1920,
            height: 620,
            color: Some("#3a7bd5".into()),
        };
        assert_eq!(
            serde_json::to_value(&media).unwrap(),
            json!({"kind": "picture", "url": "/game-assets/aa.png", "width": 1920,
                "height": 620, "color": "#3a7bd5"})
        );
        assert_eq!(
            serde_json::to_value(SurfaceMedia {
                kind: SurfaceMediaKind::Logo,
                color: None,
                ..media.clone()
            })
            .unwrap(),
            json!({"kind": "logo", "url": "/game-assets/aa.png", "width": 1920, "height": 620})
        );
        assert_eq!(
            SurfaceMediaKind::ALL.map(|kind| serde_json::to_value(kind).unwrap()),
            [json!("logo"), json!("picture"), json!("icon")]
        );
        for kind in SurfaceMediaKind::ALL {
            assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.as_str()));
        }
        // The fixed roles are gone, with no alias (ADR 0091): a tile, a
        // poster or a hero is not a kind, and `role` does not stand in for `kind`.
        for role in ["tile", "poster", "hero", "Picture"] {
            assert!(
                serde_json::from_value::<SurfaceMediaKind>(json!(role)).is_err(),
                "{role}"
            );
        }
        for item in [
            json!({"role": "logo", "url": "/game-assets/aa.png", "width": 4, "height": 4}),
            json!({"kind": "tile", "url": "/game-assets/aa.png", "width": 4, "height": 4}),
        ] {
            assert!(
                serde_json::from_value::<SurfaceMedia>(item.clone()).is_err(),
                "{item}"
            );
        }
        assert_eq!(
            serde_json::to_value(SurfaceSystem {
                system_id: "gba".into(),
                system: "Game Boy Advance".into(),
                total: 3
            })
            .unwrap(),
            json!({"systemId": "gba", "system": "Game Boy Advance", "total": 3})
        );
        // `publishedAt` is a number of milliseconds, as `lastPlayedAt` is.
        let model: SurfaceModel = serde_json::from_value(json!({
            "presentation": {"kind": "catalog"}, "catalog": {"_tag": "Loading"},
            "status": {"_tag": "Browsing"}, "actions": [], "settings": [],
            "settingsStatus": {"_tag": "Idle"}, "publishedAt": 1_794_446_934_334.0_f64,
            "systems": [{"systemId": "gba", "system": "Game Boy Advance", "total": 3}]
        }))
        .unwrap();
        assert_eq!(model.published_at, Some(1_794_446_934_334.0));
        let wire = serde_json::to_value(&model).unwrap();
        assert_eq!(wire["publishedAt"], json!(1_794_446_934_334.0_f64));
        assert_eq!(wire["systems"][0]["systemId"], json!("gba"));
        let query = CatalogQuery {
            system_id: Some("gba".into()),
            ..CatalogQuery::default()
        };
        assert_eq!(
            serde_json::to_value(&query).unwrap(),
            json!({"text": "", "systemId": "gba", "order": "korri"})
        );
        // A package written before the field existed still asks for every
        // system, and a game without the new facts carries none of them.
        assert_eq!(
            serde_json::from_value::<CatalogQuery>(json!({"text": "", "order": "korri"})).unwrap(),
            CatalogQuery::default()
        );
        let game: SurfaceGame = serde_json::from_value(json!({"id": "g", "title": "G"})).unwrap();
        assert_eq!(
            (game.system_id, game.system, game.media),
            (None, None, None)
        );
        let game: SurfaceGame = serde_json::from_value(json!({
            "id": "g", "title": "G", "systemId": "gba", "system": "Game Boy Advance",
            "media": [{"kind": "picture", "url": "/game-assets/bb.jpg", "width": 600, "height": 900}]
        }))
        .unwrap();
        assert_eq!(game.system_id.as_deref(), Some("gba"));
        assert_eq!(game.system.as_deref(), Some("Game Boy Advance"));
        assert_eq!(game.media.unwrap()[0].kind, SurfaceMediaKind::Picture);
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
            json!({"_tag": "Error", "message": "m", "groups": [], "changes": [], "profiles": [], "notes": [], "actions": []})
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

    /// The JSON that typeshare.toml's `SurfaceAccountStateWire` spells, in
    /// a `SurfaceAccountSignIn`, and the model field that carries them.
    #[test]
    fn account_sign_in_wire() {
        let action = SurfaceAction::new("account-sign-in:cancel:@korri:steam", "Cancel", true);
        let code = SurfaceAccountState::Code {
            qr: SurfaceQrCode {
                size: 21,
                modules: "1".repeat(441),
            },
            kicker: "Sign in to Steam".into(),
            detail: None,
            for_game: Some("Celeste".into()),
            actions: vec![action.clone()],
        };
        assert_eq!(
            serde_json::to_value(&code).unwrap(),
            json!({"_tag": "Code", "qr": {"size": 21, "modules": "1".repeat(441)},
                "kicker": "Sign in to Steam", "forGame": "Celeste",
                "actions": [{"id": "account-sign-in:cancel:@korri:steam", "label": "Cancel", "enabled": true}]})
        );
        assert_eq!(
            serde_json::to_value(SurfaceAccountState::SignedIn {
                account_name: "p".into(),
                actions: vec![],
            })
            .unwrap(),
            json!({"_tag": "SignedIn", "accountName": "p", "actions": []})
        );
        assert_eq!(
            serde_json::to_value(SurfaceAccountState::Working {
                kicker: "k".into(),
                for_game: None,
                actions: vec![],
            })
            .unwrap(),
            json!({"_tag": "Working", "kicker": "k", "actions": []})
        );
        assert_eq!(
            serde_json::to_value(SurfaceAccountState::SignedOut {
                kicker: None,
                actions: vec![],
            })
            .unwrap(),
            json!({"_tag": "SignedOut", "actions": []})
        );
        assert_eq!(
            serde_json::to_value(SurfaceAccountState::Problem {
                kicker: "k".into(),
                reason: "r".into(),
                actions: vec![],
            })
            .unwrap(),
            json!({"_tag": "Problem", "kicker": "k", "reason": "r", "actions": []})
        );
        let round: SurfaceAccountState =
            serde_json::from_value(serde_json::to_value(&code).unwrap()).unwrap();
        assert_eq!(round, code);

        let expired = SurfaceAccountSignIn {
            id: "@korri:tailscale".into(),
            title: "Tailscale".into(),
            logo: Some(SurfaceMedia {
                kind: SurfaceMediaKind::Logo,
                url: "data:image/svg+xml;base64,PHN2Zy8+".into(),
                width: 24,
                height: 24,
                color: None,
            }),
            offer: Some("Private network: reach your devices from anywhere".into()),
            state: SurfaceAccountState::SignedOut {
                kicker: Some("Sign in to Tailscale again".into()),
                actions: vec![action],
            },
        };
        let wire = json!({"id": "@korri:tailscale", "title": "Tailscale",
            "logo": {"kind": "logo", "url": "data:image/svg+xml;base64,PHN2Zy8+", "width": 24, "height": 24},
            "offer": "Private network: reach your devices from anywhere",
            "state": {"_tag": "SignedOut", "kicker": "Sign in to Tailscale again",
                "actions": [{"id": "account-sign-in:cancel:@korri:steam", "label": "Cancel", "enabled": true}]}});
        assert_eq!(serde_json::to_value(&expired).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<SurfaceAccountSignIn>(wire).unwrap(),
            expired
        );
        let bare = SurfaceAccountSignIn {
            id: "s".into(),
            title: "Steam".into(),
            logo: None,
            offer: None,
            state: SurfaceAccountState::SignedOut {
                kicker: None,
                actions: vec![],
            },
        };
        assert_eq!(
            serde_json::to_value(&bare).unwrap(),
            json!({"id": "s", "title": "Steam", "state": {"_tag": "SignedOut", "actions": []}})
        );
    }

    /// The JSON that typeshare.toml's install wires spell (ADR 0092).
    #[test]
    fn install_wire() {
        assert_eq!(
            serde_json::to_value(SurfaceInstallHere::Offered).unwrap(),
            json!({"_tag": "Offered"})
        );
        assert_eq!(
            serde_json::to_value(SurfaceInstallHere::NeedsPlugin {
                plugin_title: "mGBA".into()
            })
            .unwrap(),
            json!({"_tag": "NeedsPlugin", "pluginTitle": "mGBA"})
        );
        assert_eq!(
            serde_json::to_value(SurfaceInstallHere::Installing {
                install_id: "i".into()
            })
            .unwrap(),
            json!({"_tag": "Installing", "installId": "i"})
        );
        assert_eq!(
            serde_json::to_value(SurfaceInstallHere::Unavailable {
                reason: SurfaceInstallBlock::NotEnoughSpace,
                bytes: Some(4.0e9),
            })
            .unwrap(),
            json!({"_tag": "Unavailable", "reason": "notEnoughSpace", "bytes": 4.0e9})
        );
        for (block, wire) in [
            (SurfaceInstallBlock::NoRunner, "noRunner"),
            (SurfaceInstallBlock::NoSource, "noSource"),
            (SurfaceInstallBlock::NotAllowed, "notAllowed"),
        ] {
            assert_eq!(serde_json::to_value(block).unwrap(), json!(wire));
        }
        let install = SurfaceInstall {
            id: "i".into(),
            title: "Anguna".into(),
            game_id: Some("g".into()),
            state: SurfaceInstallState::Installing {
                progress: SurfaceProgress {
                    done: 1.0,
                    total: Some(4.0),
                },
                seconds_left: Some(300.0),
            },
            play_when_ready: Some(SurfaceTrue),
            stream_meanwhile: Some(SurfaceTrue),
        };
        let wire = json!({"id": "i", "title": "Anguna", "gameId": "g",
            "state": {"_tag": "Installing", "progress": {"done": 1.0, "total": 4.0}, "secondsLeft": 300.0},
            "playWhenReady": true, "streamMeanwhile": true});
        assert_eq!(serde_json::to_value(&install).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<SurfaceInstall>(wire).unwrap(),
            install
        );
        assert_eq!(
            serde_json::to_value(SurfaceInstallState::Waiting { bytes: None }).unwrap(),
            json!({"_tag": "Waiting"})
        );
        assert_eq!(
            serde_json::to_value(SurfaceInstallState::Failed { reason: "r".into() }).unwrap(),
            json!({"_tag": "Failed", "reason": "r"})
        );
        for (request, wire) in [
            (
                SurfaceRequest::InstallGame {
                    game_id: "g".into(),
                    play: true,
                },
                json!({"_tag": "InstallGame", "gameId": "g", "play": true}),
            ),
            (
                SurfaceRequest::MoveInstall {
                    install_id: "i".into(),
                    index: 0,
                },
                json!({"_tag": "MoveInstall", "installId": "i", "index": 0}),
            ),
            (
                SurfaceRequest::CancelInstall {
                    install_id: "i".into(),
                },
                json!({"_tag": "CancelInstall", "installId": "i"}),
            ),
            (
                SurfaceRequest::AnswerStreamMeanwhile {
                    install_id: "i".into(),
                    stream: false,
                    launch_location_id: None,
                },
                json!({"_tag": "AnswerStreamMeanwhile", "installId": "i", "stream": false}),
            ),
        ] {
            assert_eq!(serde_json::to_value(&request).unwrap(), wire);
            assert_eq!(
                serde_json::from_value::<SurfaceRequest>(wire).unwrap(),
                request
            );
        }
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
