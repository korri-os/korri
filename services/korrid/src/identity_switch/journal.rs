use crate::identity::{DeviceIdentity, OwnerStatementStatus};
use serde::{Deserialize, Serialize};

pub const MAX_JOURNAL_BYTES: usize = 192 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JournalPhase {
    Preparing,
    Commit,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DataDisposition {
    Transfer,
    Delete,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplacementSigner {
    Local,
    Nip46,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentitySwitchJournal {
    phase: JournalPhase,
    data_disposition: DataDisposition,
    replacement_signer: ReplacementSigner,
    old_owner_revocation: String,
    old_owner_was_published: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum JournalError {
    #[error("identity-switch journal exceeds its size bound")]
    TooLarge,
    #[error("identity-switch journal is malformed")]
    Malformed,
    #[error("identity-switch journal owner revocation is invalid")]
    InvalidRevocation,
}

impl IdentitySwitchJournal {
    pub fn new(
        phase: JournalPhase,
        data_disposition: DataDisposition,
        replacement_signer: ReplacementSigner,
        old_owner_revocation: String,
        old_owner_was_published: bool,
    ) -> Result<Self, JournalError> {
        let journal = Self {
            phase,
            data_disposition,
            replacement_signer,
            old_owner_revocation,
            old_owner_was_published,
        };
        journal.validate()?;
        Ok(journal)
    }

    pub fn phase(&self) -> JournalPhase {
        self.phase
    }

    pub fn data_disposition(&self) -> DataDisposition {
        self.data_disposition
    }

    pub fn replacement_signer(&self) -> ReplacementSigner {
        self.replacement_signer
    }

    pub fn old_owner_revocation(&self) -> &str {
        &self.old_owner_revocation
    }

    pub fn old_owner_was_published(&self) -> bool {
        self.old_owner_was_published
    }

    pub fn committing(&self) -> Self {
        Self {
            phase: JournalPhase::Commit,
            ..self.clone()
        }
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, JournalError> {
        self.validate()?;
        let encoded = serde_json::to_vec(self).map_err(|_| JournalError::Malformed)?;
        if encoded.len() > MAX_JOURNAL_BYTES {
            return Err(JournalError::TooLarge);
        }
        Ok(encoded)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, JournalError> {
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(JournalError::TooLarge);
        }
        let journal: Self = serde_json::from_slice(bytes).map_err(|_| JournalError::Malformed)?;
        journal.validate()?;
        Ok(journal)
    }

    fn validate(&self) -> Result<(), JournalError> {
        if self.old_owner_revocation.len() > MAX_JOURNAL_BYTES {
            return Err(JournalError::TooLarge);
        }
        let statement = DeviceIdentity::derive_owner_statement(&self.old_owner_revocation)
            .map_err(|_| JournalError::InvalidRevocation)?;
        if statement.status != OwnerStatementStatus::Revoked {
            return Err(JournalError::InvalidRevocation);
        }
        Ok(())
    }
}
