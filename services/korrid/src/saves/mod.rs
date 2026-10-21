//! Session-bound save history. No filesystem path crosses the peer boundary.
mod history;
pub mod peer;
pub mod record;
pub mod store;
pub mod types;

use std::{fs::File, io::Read};
use types::{SaveHeads, SnapshotRecord};

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("save storage is unavailable: {0}")]
    Storage(String),
    #[error("invalid save data: {0}")]
    Invalid(String),
    #[error("save content is missing")]
    Missing,
    #[error("saves split between devices; choose one before launch")]
    Split,
    #[error("save locations cannot change while the game is live")]
    InUse,
}

/// Native persistence boundary shared by session lifecycle and peer transfer.
/// A record may arrive before its objects. It is confirmed only after every
/// referenced object is present and verified. All operations are person-scoped;
/// object reads must prove the hash occurs in that person's stored records.
/// Implementations serialize mutation and apply the play log's private-file rules.
pub trait SaveRepository: Send + Sync {
    fn heads(&self, person: &str) -> Result<SaveHeads, SaveError>;
    fn records(&self, person: &str, game: &str) -> Result<Vec<SnapshotRecord>, SaveError>;
    /// Validates and canonicalizes the immutable record; returns its content id.
    fn put_record(&self, person: &str, record: &SnapshotRecord) -> Result<String, SaveError>;
    /// Returns a verified regular file, positioned at byte zero for Range reads.
    fn object(&self, person: &str, hash: &str) -> Result<File, SaveError>;
    /// Streams into a private temporary and publishes only after hash validation.
    /// The hash must already be named by one of this person's stored records.
    fn put_object(&self, person: &str, hash: &str, input: &mut dyn Read) -> Result<(), SaveError>;
    fn complete(&self, person: &str, game: &str, id: &str) -> Result<bool, SaveError>;
    /// Persists an authenticated peer's acknowledgement, never inferred by time.
    fn confirm(&self, person: &str, game: &str, id: &str, device: &str) -> Result<(), SaveError>;
}

/// The owner key is the account identity, using the existing play-log codec.
pub fn account_root(root: &std::path::Path, person: &str) -> Result<std::path::PathBuf, String> {
    if person.len() != 64
        || !person
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("device owner public key is invalid".into());
    }
    Ok(root
        .join("users")
        .join(crate::host::play_log::encode_uri_component(person)))
}

/// The one-time local side of first sync. Migrated files must enter history
/// before any received snapshot may replace them at a launch boundary.
pub(crate) fn capture_initial(
    store: &store::NativeSaveStore,
    person: &str,
    locations: &[types::SaveLocation],
    template: types::SnapshotRecord,
) -> Result<(), SaveError> {
    if store
        .records(person, &template.game_id)?
        .iter()
        .any(|r| r.device == template.device)
    {
        return Ok(());
    }
    let record = store.capture(person, locations, template)?;
    if record.locations.iter().any(|l| !l.files.is_empty()) {
        store.put_record(person, &record)?;
    }
    Ok(())
}
