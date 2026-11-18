//! `@korri:steam` as a provider (ADR 0083, contract "Provider and installs").
//!
//! The list is the host's `list-apps` output for the current person: one
//! `app {appid, name, source}` per app, then `apps_done`. korrid serves
//! search from it. Get runs `install <appid>` in job mode and Remove
//! download runs `uninstall <appid>`; progress comes from `app_progress`.
//! A finished install is found by discovery from its appmanifest, as for
//! the client runner.
use super::protocol::{AppId, AppSource, Event};
use korri_surface_treaty::{SurfaceProgress, SurfaceStoreEntry};
use std::collections::BTreeMap;

/// The provider id the store shows.
pub const PROVIDER_ID: &str = "@korri:steam";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SteamApp {
    pub appid: AppId,
    pub name: String,
    pub source: AppSource,
}

/// Collects one `list-apps` answer.
#[derive(Debug, Default)]
pub struct AppList {
    apps: BTreeMap<AppId, SteamApp>,
    done: bool,
}

impl AppList {
    /// Feed one event; true once the list is complete.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::App {
                appid,
                name,
                source,
            } => {
                let name = name.trim();
                if !name.is_empty() && !name.chars().any(char::is_control) {
                    // An app can be owned and shared at once; the person's own
                    // licence names it first.
                    let entry = self.apps.entry(*appid).or_insert(SteamApp {
                        appid: *appid,
                        name: name.into(),
                        source: *source,
                    });
                    if *source < entry.source {
                        entry.source = *source;
                    }
                }
            }
            Event::AppsDone {} => self.done = true,
            _ => {}
        }
        self.done
    }

    pub fn done(&self) -> bool {
        self.done
    }

    pub fn apps(&self) -> impl Iterator<Item = &SteamApp> {
        self.apps.values()
    }

    /// Entries whose name holds every word of `query`, case-folded, by name.
    /// An empty query lists everything: the store's "your Steam games".
    pub fn search(&self, query: &str) -> Vec<SurfaceStoreEntry> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut found: Vec<&SteamApp> = self
            .apps
            .values()
            .filter(|app| {
                let name = app.name.to_lowercase();
                words.iter().all(|word| name.contains(word))
            })
            .collect();
        found.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.appid.cmp(&b.appid))
        });
        found
            .into_iter()
            .map(|app| SurfaceStoreEntry {
                id: app.appid.to_string(),
                title: app.name.clone(),
                system: Some(
                    match app.source {
                        AppSource::Owned => "Steam",
                        AppSource::Free => "Steam · Free",
                        AppSource::Family => "Steam · Family",
                    }
                    .into(),
                ),
            })
            .collect()
    }
}

/// Parse a store entry id back to an app id.
pub fn appid(entry: &str) -> Option<AppId> {
    (!entry.is_empty() && entry.len() <= 10 && entry.bytes().all(|b| b.is_ascii_digit()))
        .then(|| entry.parse().ok())
        .flatten()
}

/// Where an install or uninstall job stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    Starting,
    Moving { done: u64, total: u64 },
    Finished { ok: bool, result: String },
}

impl Job {
    /// The next state after one host event for `appid`.
    pub fn apply(&self, appid: AppId, event: &Event) -> Self {
        match event {
            Event::AppProgress {
                appid: id,
                bytes_downloaded,
                bytes_to_download,
                bytes_staged,
                bytes_to_stage,
            } if *id == appid => Self::Moving {
                done: bytes_downloaded.saturating_add(*bytes_staged),
                total: bytes_to_download.saturating_add(*bytes_to_stage),
            },
            Event::AppJobFinished {
                appid: id, result, ..
            } if *id == appid => Self::Finished {
                ok: super::protocol::app_job_succeeded(result),
                result: result.clone(),
            },
            _ => self.clone(),
        }
    }

    /// Bytes downloaded and staged: Steam's own two phases as one bar.
    pub fn progress(&self) -> Option<SurfaceProgress> {
        match self {
            Self::Moving { done, total } if *total > 0 => Some(SurfaceProgress {
                done: *done as f64,
                total: Some(*total as f64),
            }),
            Self::Moving { done, .. } => Some(SurfaceProgress {
                done: *done as f64,
                total: None,
            }),
            _ => None,
        }
    }

    /// Finished, user-facing copy for a failed job.
    pub fn failure(result: &str) -> &'static str {
        match result {
            "install_refused" => "Steam refused to install this game on this device.",
            "install_failed" => {
                "Steam stopped the download. Check the space and the network, then try again."
            }
            "offline" => "Steam is offline. Connect to the network and try again.",
            "timeout" => "The download took too long. Try again.",
            "uninstall_refused" => "Steam refused to remove this game.",
            _ => "Steam did not finish. Try again.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(appid: AppId, name: &str, source: AppSource) -> Event {
        Event::App {
            appid,
            name: name.into(),
            source,
        }
    }

    #[test]
    fn the_list_holds_owned_free_and_family_apps_and_search_matches_words() {
        let mut list = AppList::default();
        assert!(!list.apply(&app(70, "Half-Life", AppSource::Family)));
        list.apply(&app(70, "Half-Life", AppSource::Owned));
        list.apply(&app(440, "Team Fortress 2", AppSource::Free));
        list.apply(&app(504230, "Celeste", AppSource::Owned));
        list.apply(&app(1, "\u{7}", AppSource::Owned));
        assert!(list.apply(&Event::AppsDone {}));
        assert_eq!(list.apps().count(), 3);
        let all = list.search("");
        assert_eq!(
            all.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
            ["Celeste", "Half-Life", "Team Fortress 2"]
        );
        assert_eq!(all[1].system.as_deref(), Some("Steam"));
        assert_eq!(all[2].system.as_deref(), Some("Steam · Free"));
        let found = list.search("fortress TEAM");
        assert_eq!(found.len(), 1);
        assert_eq!(appid(&found[0].id), Some(440));
        assert_eq!(appid("../1"), None);
        assert_eq!(appid("99999999999"), None);
    }

    #[test]
    fn an_install_job_reports_both_phases_as_one_bar_and_its_result() {
        let mut job = Job::Starting;
        assert_eq!(job.progress(), None);
        job = job.apply(
            78000,
            &Event::AppProgress {
                appid: 78000,
                bytes_downloaded: 40,
                bytes_to_download: 80,
                bytes_staged: 10,
                bytes_to_stage: 100,
            },
        );
        assert_eq!(
            job.progress(),
            Some(SurfaceProgress {
                done: 50.0,
                total: Some(180.0)
            })
        );
        // Another app's progress is not this job's.
        let other = job.apply(
            78000,
            &Event::AppJobFinished {
                appid: 1,
                kind: "install".into(),
                result: "installed".into(),
            },
        );
        assert_eq!(other, job);
        let done = job.apply(
            78000,
            &Event::AppJobFinished {
                appid: 78000,
                kind: "install".into(),
                result: "install_failed".into(),
            },
        );
        assert_eq!(
            done,
            Job::Finished {
                ok: false,
                result: "install_failed".into()
            }
        );
        assert!(Job::failure("install_failed").contains("space"));
    }
}
