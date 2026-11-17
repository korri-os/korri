//! Crash-recoverable storage primitives for an explicit device identity switch.
//!
//! The strict journal and Linux directory exchange provide the durable boundary.
//! The coordinator orders signer, relay, trust, and person-data effects around it.

mod coordinator;
mod journal;
mod storage;

#[cfg(test)]
pub(crate) use coordinator::revoke_stream_client_trust;
pub use coordinator::{
    recover_pending_identity_switch, IdentitySwitchCoordinator, PreparedSwitch, ReplacementIdentity,
};
pub use journal::{DataDisposition, IdentitySwitchJournal, JournalError, JournalPhase};
pub use storage::{
    ExchangePosition, IdentityInspection, IdentitySwitchStorage, IdentitySwitchStorageError,
};

#[cfg(test)]
mod tests;
