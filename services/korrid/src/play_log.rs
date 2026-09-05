use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use typeshare::typeshare;

use serde::{Deserialize, Serialize};

pub const DEFAULT_PLAY_LOG_THRESHOLD_SECONDS: u64 = 0;
const PLAY_LOGS_DIR: &str = "play-logs";

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayEntry {
    /// When the session occurred (its end time, in UTC ISO 8601).
    pub occurred_at: String,
    /// How long the session lasted, in seconds. u32 because Typeshare has no
    /// u64; 136 years per session is enough.
    pub duration_seconds: u32,
    /// Release the session was launched from, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayStats {
    /// UTC timestamp of newest session, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played: Option<String>,
    pub play_count: u32,
    pub total_playtime_seconds: u32,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayLog {
    pub game_id: String,
    pub entries: Vec<PlayEntry>,
}

pub fn derive_play_stats(entries: &[PlayEntry]) -> PlayStats {
    if entries.is_empty() {
        return PlayStats {
            last_played: None,
            play_count: 0,
            total_playtime_seconds: 0,
        };
    }

    let mut newest = &entries[0].occurred_at;
    let mut total_playtime_seconds: u32 = 0;

    for entry in entries {
        if entry.occurred_at.as_str() > newest.as_str() {
            newest = &entry.occurred_at;
        }
        total_playtime_seconds = total_playtime_seconds.saturating_add(entry.duration_seconds);
    }

    PlayStats {
        last_played: Some(newest.clone()),
        play_count: entries.len() as u32,
        total_playtime_seconds,
    }
}

pub fn qualifies_for_play_log(duration_seconds: u64, threshold_seconds: u64) -> bool {
    duration_seconds >= threshold_seconds
}

/// Convert seconds since UNIX epoch to an ISO 8601 UTC string (e.g. "2026-09-04T18:30:00Z").
pub fn format_iso8601_utc(epoch_seconds: u64) -> String {
    let secs = epoch_seconds % 60;
    let mins = (epoch_seconds / 60) % 60;
    let hours = (epoch_seconds / 3600) % 24;
    let mut days = (epoch_seconds / 86400) as i64;

    // Euclidean affine algorithm for civil date from day number (Howard Hinnant)
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hours, mins, secs
    )
}

pub fn current_iso8601_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_iso8601_utc(seconds)
}

#[derive(Clone, Debug)]
pub struct PlayLogRepository {
    directory: PathBuf,
    threshold_seconds: u64,
}

impl PlayLogRepository {
    pub fn new(private_state_root: &Path) -> Self {
        Self::with_threshold(private_state_root, DEFAULT_PLAY_LOG_THRESHOLD_SECONDS)
    }

    pub fn with_threshold(private_state_root: &Path, threshold_seconds: u64) -> Self {
        Self {
            directory: private_state_root.join(PLAY_LOGS_DIR),
            threshold_seconds,
        }
    }

    fn file_path(&self, game_id: &str) -> PathBuf {
        let hex_name = hex::encode(game_id.as_bytes());
        self.directory.join(format!("{hex_name}.json"))
    }

    pub fn load_log(&self, game_id: &str) -> Result<PlayLog, io::Error> {
        let path = self.file_path(game_id);
        if !path.exists() {
            return Ok(PlayLog {
                game_id: game_id.to_owned(),
                entries: Vec::new(),
            });
        }
        let bytes = fs::read(&path)?;
        let log: PlayLog = serde_json::from_slice(&bytes)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        Ok(log)
    }

    pub fn load_stats(&self, game_id: &str) -> PlayStats {
        self.load_log(game_id)
            .map(|log| derive_play_stats(&log.entries))
            .unwrap_or_else(|_| PlayStats {
                last_played: None,
                play_count: 0,
                total_playtime_seconds: 0,
            })
    }

    pub fn record_session(
        &self,
        game_id: &str,
        occurred_at: &str,
        duration_seconds: u64,
        release_id: Option<&str>,
    ) -> Result<PlayStats, io::Error> {
        if !qualifies_for_play_log(duration_seconds, self.threshold_seconds) {
            return Ok(self.load_stats(game_id));
        }
        let duration_seconds = u32::try_from(duration_seconds).unwrap_or(u32::MAX);

        fs::create_dir_all(&self.directory)?;
        let mut log = self.load_log(game_id)?;
        log.entries.push(PlayEntry {
            occurred_at: occurred_at.to_owned(),
            duration_seconds,
            release_id: release_id.map(String::from),
        });

        let stats = derive_play_stats(&log.entries);
        let bytes = serde_json::to_vec_pretty(&log)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        write_atomically(&self.file_path(game_id), &bytes)?;

        Ok(stats)
    }

    pub fn load_all_stats(&self) -> BTreeMap<String, PlayStats> {
        let mut results = BTreeMap::new();
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(_) => return results,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if let Ok(bytes) = fs::read(&path) {
                if let Ok(log) = serde_json::from_slice::<PlayLog>(&bytes) {
                    let stats = derive_play_stats(&log.entries);
                    results.insert(log.game_id, stats);
                }
            }
        }

        results
    }
}

fn write_atomically(path: &Path, content: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "missing parent directory"))?;
    let temp_name = format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("log"),
        rand::random::<u64>()
    );
    let temp_path = parent.join(temp_name);

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)?;

    if let Err(err) = file.write_all(content).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }
    drop(file);

    if let Err(err) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }

    let _ = File::open(parent).and_then(|dir| dir.sync_all());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn empty_entries_yield_empty_stats() {
        let stats = derive_play_stats(&[]);
        assert_eq!(stats.last_played, None);
        assert_eq!(stats.play_count, 0);
        assert_eq!(stats.total_playtime_seconds, 0);
    }

    #[test]
    fn single_entry_derives_stats() {
        let entries = vec![PlayEntry {
            occurred_at: "2026-09-01T12:00:00Z".into(),
            duration_seconds: 300,
            release_id: None,
        }];
        let stats = derive_play_stats(&entries);
        assert_eq!(stats.last_played, Some("2026-09-01T12:00:00Z".into()));
        assert_eq!(stats.play_count, 1);
        assert_eq!(stats.total_playtime_seconds, 300);
    }

    #[test]
    fn multiple_entries_take_newest_and_sum_duration() {
        let entries = vec![
            PlayEntry {
                occurred_at: "2026-08-15T10:00:00Z".into(),
                duration_seconds: 600,
                release_id: Some("v1".into()),
            },
            PlayEntry {
                occurred_at: "2026-09-04T18:00:00Z".into(),
                duration_seconds: 1200,
                release_id: Some("v2".into()),
            },
            PlayEntry {
                occurred_at: "2026-09-01T14:30:00Z".into(),
                duration_seconds: 400,
                release_id: None,
            },
        ];
        let stats = derive_play_stats(&entries);
        assert_eq!(stats.last_played, Some("2026-09-04T18:00:00Z".into()));
        assert_eq!(stats.play_count, 3);
        assert_eq!(stats.total_playtime_seconds, 2200);
    }

    #[test]
    fn threshold_qualifies_sessions() {
        assert!(qualifies_for_play_log(0, 0));
        assert!(qualifies_for_play_log(10, 0));
        assert!(qualifies_for_play_log(60, 60));
        assert!(!qualifies_for_play_log(59, 60));
    }

    #[test]
    fn format_iso8601_utc_matches_known_dates() {
        assert_eq!(format_iso8601_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_iso8601_utc(86400), "1970-01-02T00:00:00Z");
        // 2026-09-04T18:30:00Z = 1788546600
        assert_eq!(format_iso8601_utc(1788546600), "2026-09-04T18:30:00Z");
    }

    #[test]
    fn repository_records_and_loads_play_logs() {
        let temp = tempdir().unwrap();
        let repo = PlayLogRepository::new(temp.path());

        assert_eq!(repo.load_stats("game-1").play_count, 0);

        let stats1 = repo
            .record_session("game-1", "2026-09-01T10:00:00Z", 150, None)
            .unwrap();
        assert_eq!(stats1.play_count, 1);
        assert_eq!(stats1.total_playtime_seconds, 150);
        assert_eq!(stats1.last_played, Some("2026-09-01T10:00:00Z".into()));

        let stats2 = repo
            .record_session("game-1", "2026-09-02T12:00:00Z", 350, Some("rel-1"))
            .unwrap();
        assert_eq!(stats2.play_count, 2);
        assert_eq!(stats2.total_playtime_seconds, 500);
        assert_eq!(stats2.last_played, Some("2026-09-02T12:00:00Z".into()));

        let loaded = repo.load_stats("game-1");
        assert_eq!(loaded, stats2);

        let all = repo.load_all_stats();
        assert_eq!(all.len(), 1);
        assert_eq!(all.get("game-1"), Some(&stats2));
    }

    #[test]
    fn repository_ignores_sessions_below_threshold() {
        let temp = tempdir().unwrap();
        let repo = PlayLogRepository::with_threshold(temp.path(), 60);

        let stats = repo
            .record_session("game-quick", "2026-09-01T10:00:00Z", 30, None)
            .unwrap();
        assert_eq!(stats.play_count, 0);
        assert_eq!(repo.load_stats("game-quick").play_count, 0);
    }
}
