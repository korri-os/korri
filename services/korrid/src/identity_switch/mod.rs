//! Crash-recoverable storage primitives for an explicit device identity switch.
//!
//! This module owns only the strict journal and one Linux directory exchange.
//! The coordinator decides policy and orders signer, relay, trust, and person-data
//! effects around these primitives.

mod journal;
mod storage;

pub use journal::{
    DataDisposition, IdentitySwitchJournal, JournalError, JournalPhase, ReplacementSigner,
};
pub use storage::{
    ExchangePosition, IdentityInspection, IdentitySwitchStorage, IdentitySwitchStorageError,
};

#[cfg(test)]
mod tests;
