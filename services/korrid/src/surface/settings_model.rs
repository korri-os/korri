//! Port of `clients/portal/src/surface/settings-model.ts`: device facts and
//! the narrow settings Korri can honestly change today.

use korri_surface_treaty::{
    SurfaceSettingChoice, SurfaceSettingConfirmation, SurfaceSettingGroup,
    SurfaceSettingInteraction, SurfaceSettingItem,
};

use crate::{
    config::settings::SecretSettingStatus, LibraryFolderStatus, LibraryFoldersSnapshot,
    LibraryFoldersState, SettingsSnapshot,
};

/// Every fact here comes from korrid.
#[derive(Clone, Debug, Default)]
pub struct DeviceFacts {
    pub version: Option<String>,
    pub settings: Option<SettingsSnapshot>,
    pub local_game_count: Option<usize>,
    pub discovery: Option<LibraryFoldersSnapshot>,
    /// Absent until Korri has read setup progress.
    pub onboarding_needed: Option<bool>,
    /// The plugin groups built from the repository facts. Absent before they
    /// are read; then no plugin group shows.
    pub plugin_groups: Option<Vec<SurfaceSettingGroup>>,
    /// This device's launch settings groups. Absent until read.
    pub launch_settings: Option<Vec<SurfaceSettingGroup>>,
}

/// Setting-local action that puts the device back into first-boot setup.
pub const SETUP_RESTART_ACTION_ID: &str = "setup-restart";

fn count_label(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

fn group(title: &str, items: Vec<Option<SurfaceSettingItem>>) -> Option<SurfaceSettingGroup> {
    let present: Vec<_> = items.into_iter().flatten().collect();
    (!present.is_empty()).then(|| SurfaceSettingGroup {
        title: title.into(),
        items: present,
    })
}

fn discovery_state_label(snapshot: Option<&LibraryFoldersSnapshot>) -> &'static str {
    match snapshot.map(|snapshot| &snapshot.state) {
        None => "Not set up",
        Some(LibraryFoldersState::Scanning {}) => "Scanning…",
        Some(LibraryFoldersState::Enriching {}) => "Adding details…",
        Some(LibraryFoldersState::Problem {}) => "Needs attention",
        Some(LibraryFoldersState::Idle {}) => "Ready",
    }
}

fn folder_status_label(status: &LibraryFolderStatus) -> &'static str {
    match status {
        LibraryFolderStatus::Ready {} => "Registered",
        LibraryFolderStatus::Overlap {} => "Overlaps another folder",
        LibraryFolderStatus::SystemUnavailable {} => "System unavailable",
    }
}

fn text(placeholder: &str, max_length: u32) -> SurfaceSettingInteraction {
    SurfaceSettingInteraction::Text {
        placeholder: Some(placeholder.into()),
        max_length: Some(max_length),
        allow_empty: None,
    }
}

fn action(
    action_id: impl Into<String>,
    destructive: Option<bool>,
    confirmation: Option<(&str, &str, &str)>,
) -> SurfaceSettingInteraction {
    SurfaceSettingInteraction::Action {
        action_id: action_id.into(),
        destructive,
        confirmation: confirmation.map(|(title, message, confirm_label)| {
            SurfaceSettingConfirmation {
                title: title.into(),
                message: message.into(),
                confirm_label: confirm_label.into(),
            }
        }),
    }
}

pub fn settings_from(facts: &DeviceFacts) -> Vec<SurfaceSettingGroup> {
    let can_edit = |id: &str| {
        facts.settings.as_ref().is_some_and(|settings| {
            settings
                .editable_setting_ids
                .iter()
                .any(|editable| editable == id)
        })
    };
    let settings = facts.settings.as_ref();
    let mut groups: Vec<Option<SurfaceSettingGroup>> = vec![
        group(
            "Device",
            vec![
                settings.map(|settings| SurfaceSettingItem {
                    // No name, no value: a surface's editor then starts empty.
                    value: settings.device_name.clone(),
                    interaction: can_edit("device-name").then(|| text("This device", 64)),
                    ..SurfaceSettingItem::new("device-name", "Name")
                }),
                settings.map(|settings| SurfaceSettingItem {
                    value: settings.person_name.clone(),
                    interaction: can_edit("person-name").then(|| text("Your name", 64)),
                    ..SurfaceSettingItem::new("person-name", "Person name")
                }),
                // Offered only once setup has finished.
                (facts.onboarding_needed == Some(false)).then(|| SurfaceSettingItem {
                    description: Some("Games, Wi-Fi and identity stay.".into()),
                    interaction: Some(action(
                        SETUP_RESTART_ACTION_ID,
                        None,
                        Some((
                            "Run setup again?",
                            "Korri will show first-boot setup now. Games, Wi-Fi and identity stay.",
                            "Run setup",
                        )),
                    )),
                    ..SurfaceSettingItem::new(SETUP_RESTART_ACTION_ID, "Run setup again")
                }),
            ],
        ),
        group(
            "Controllers",
            vec![settings.map(|settings| SurfaceSettingItem {
                value: Some(settings.player_count.to_string()),
                description: Some(
                    "Shared by physical and remote controllers. End the session before changing."
                        .into(),
                ),
                interaction: can_edit("host.preferences.playerCount").then(|| {
                    SurfaceSettingInteraction::Choice {
                        // The device-only setting accepts the positive-u8 seat range.
                        choices: (1..=255)
                            .map(|count: u32| SurfaceSettingChoice {
                                value: count.to_string(),
                                label: count.to_string(),
                            })
                            .collect(),
                        selected: settings.player_count.to_string(),
                    }
                }),
                ..SurfaceSettingItem::new("host.preferences.playerCount", "Seats")
            })],
        ),
    ];
    groups.extend(facts.launch_settings.iter().flatten().cloned().map(Some));
    groups.push(group(
        "Metadata",
        vec![settings.map(|settings| {
            let configured = settings.steam_grid_db_credential == SecretSettingStatus::Configured;
            SurfaceSettingItem {
                value: Some(
                    if configured {
                        "Configured"
                    } else {
                        "Not configured"
                    }
                    .into(),
                ),
                description: Some("Used only by korrid for metadata and cover art lookup".into()),
                interaction: can_edit("steamgriddb-credential").then(|| {
                    SurfaceSettingInteraction::SensitiveText {
                        placeholder: Some("Paste API key".into()),
                        max_length: Some(256),
                        clear_label: configured.then(|| "Clear saved key".into()),
                    }
                }),
                ..SurfaceSettingItem::new("steamgriddb-credential", "SteamGridDB API key")
            }
        })],
    ));
    // Plugins: one group per repository, built from korrid's repository facts.
    groups.extend(facts.plugin_groups.iter().flatten().cloned().map(Some));
    let discovery = facts.discovery.as_ref();
    let mut games = vec![
        facts.local_game_count.map(|count| SurfaceSettingItem {
            value: Some(count_label(count, "game")),
            description: Some("Declared in catalog/library.yaml".into()),
            ..SurfaceSettingItem::new("local-games", "On this device")
        }),
        discovery.map(|snapshot| SurfaceSettingItem {
            value: Some(discovery_state_label(Some(snapshot)).into()),
            description: Some(
                snapshot
                    .diagnostics
                    .first()
                    .map(|diagnostic| diagnostic.message.clone())
                    .unwrap_or_else(|| "Games appear as soon as scanning finds them".into()),
            ),
            ..SurfaceSettingItem::new("game-discovery-status", "Folder scan")
        }),
        discovery.map(|snapshot| SurfaceSettingItem {
            value: Some(count_label(snapshot.folders.len(), "folder")),
            interaction: Some(action("library-folder-rescan", None, None)),
            ..SurfaceSettingItem::new("library-folder-rescan", "Rescan library folders")
        }),
    ];
    for location in discovery
        .map(|snapshot| snapshot.folders.as_slice())
        .unwrap_or(&[])
    {
        games.push(Some(SurfaceSettingItem {
            value: Some(folder_status_label(&location.status).into()),
            interaction: Some(action(
                format!("library-folder-remove:{}", location.id),
                Some(true),
                Some((
                    "Remove library folder?",
                    "Korri will remove games it added from this folder. Edited or hand-authored games stay.",
                    "Remove library folder",
                )),
            )),
            ..SurfaceSettingItem::new(format!("library-folder:{}", location.id), location.label.clone())
        }));
    }
    groups.push(group("Games", games));
    groups.push(group(
        "System information",
        vec![facts.version.as_ref().map(|version| SurfaceSettingItem {
            value: Some(version.clone()),
            ..SurfaceSettingItem::new("korrid-version", "korrid")
        })],
    ));
    groups.into_iter().flatten().collect()
}
