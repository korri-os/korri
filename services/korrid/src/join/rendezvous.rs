//! Where and how two devices meet on public Nostr relays during a join
//! (ADR 0070, ADR 0074). The meeting place comes from the nameplate alone,
//! never from the secret. Every message is a stored, addressable event, so
//! a device that is offline for a moment still finds it, and each one
//! expires with its code (NIP-40).
//!
//! The owned device first marks its nameplate as in use with an open event.
//! Every later message is NIP-44 encrypted from one device key to the
//! other. None carries a recipient tag, so neither device's coordination
//! reader ever sees one.

use super::{code::Nameplate, JoiningDevice};
use crate::{
    identity::{DeviceIdentity, OWNER_EVENT_KIND},
    relay::{
        DeviceAuthSigner, RelayAuthSigner, RelayFilter, RelayList, RelayTransport, MAX_READ_EVENTS,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};

/// The join's events use the same NIP-78 application kind as the owner
/// statement and the endpoint record.
const JOIN_EVENT_KIND: u16 = OWNER_EVENT_KIND;
const IDENTIFIER_PREFIX: &str = "org.korri.join:";
/// How far two devices' clocks may differ when they read each other's
/// events.
pub(crate) const CLOCK_SKEW_SECONDS: u64 = 60;

#[derive(Debug, thiserror::Error)]
pub(crate) enum RendezvousError {
    #[error("no relay accepted the join message")]
    Publish,
    #[error("no relay could be read")]
    Read,
    #[error("the join message could not be signed")]
    Identity,
}

/// One step of the conversation after the open event. The step names the
/// event's identifier, so a newer message never replaces an older step.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase", deny_unknown_fields)]
pub(crate) enum Message {
    /// The new device found the open event and asks to join.
    Hello { open: String },
    /// The owned device's CPace message, which carries the session nonce.
    /// It answers one hello, so a device never takes an offer that an
    /// earlier attempt of its own received.
    Offer { hello: String, message: String },
    /// The new device's CPace message, its confirmation, and what the owned
    /// device shows the person before Accept.
    Answer {
        session: String,
        message: String,
        confirmation: String,
        device: JoiningDevice,
    },
    /// The owned device's confirmation.
    Confirm {
        session: String,
        confirmation: String,
    },
    /// The new device's confirmation did not match. The code is spent.
    Mismatch { session: String },
    /// The person accepted: the new device's owner statement, a vouch chain.
    Vouch {
        session: String,
        owner_statement: String,
    },
    /// The person refused the device.
    Refuse { session: String },
}

impl Message {
    pub(crate) fn step(&self) -> Step {
        match self {
            Self::Hello { .. } => Step::Hello,
            Self::Offer { .. } => Step::Offer,
            Self::Answer { .. } => Step::Answer,
            Self::Confirm { .. } => Step::Confirm,
            Self::Mismatch { .. } => Step::Mismatch,
            Self::Vouch { .. } => Step::Vouch,
            Self::Refuse { .. } => Step::Refuse,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Step {
    Hello,
    Offer,
    Answer,
    Confirm,
    Mismatch,
    Vouch,
    Refuse,
}

impl Step {
    fn name(self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::Offer => "offer",
            Self::Answer => "answer",
            Self::Confirm => "confirm",
            Self::Mismatch => "mismatch",
            Self::Vouch => "vouch",
            Self::Refuse => "refuse",
        }
    }
}

/// The open event's identifier: the meeting place of one nameplate.
pub(crate) fn open_identifier(nameplate: Nameplate) -> String {
    format!("{IDENTIFIER_PREFIX}{}", nameplate.digits())
}

fn message_identifier(nameplate: Nameplate, step: Step) -> String {
    format!("{}:{}", open_identifier(nameplate), step.name())
}

/// One message as it arrived: its event id, its author and its content.
pub(crate) struct Received {
    pub id: String,
    pub author: String,
    pub message: Message,
}

/// An open event that a relay holds for one nameplate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Open {
    pub id: String,
    pub owned_device: String,
    pub created_at: u64,
}

/// One device's view of the relays during one join.
pub(crate) struct Rendezvous {
    relays: RelayList,
    transport: Arc<dyn RelayTransport>,
    identity: Arc<DeviceIdentity>,
}

impl Rendezvous {
    pub(crate) fn new(
        relays: RelayList,
        transport: Arc<dyn RelayTransport>,
        identity: DeviceIdentity,
    ) -> Self {
        Self {
            relays,
            transport,
            identity: Arc::new(identity),
        }
    }

    pub(crate) fn identity(&self) -> &DeviceIdentity {
        &self.identity
    }

    /// The nameplates whose open events have not expired yet.
    pub(crate) async fn nameplates_in_use(
        &self,
        now: u64,
        lifetime: u64,
    ) -> Result<BTreeSet<Nameplate>, RendezvousError> {
        let filter = RelayFilter {
            identifiers: Nameplate::all().map(open_identifier).collect(),
            ..self.filter(None, now.saturating_sub(lifetime + CLOCK_SKEW_SECONDS))
        };
        Ok(self
            .read(&filter)
            .await?
            .iter()
            .filter_map(|event_json| {
                let event = DeviceIdentity::verify_event(event_json).ok()?;
                if !is_current(&event.tags, now) {
                    return None;
                }
                let identifier = event
                    .tags
                    .first()
                    .filter(|tag| tag.len() == 2 && tag[0] == "d")?;
                Nameplate::parse(identifier[1].strip_prefix(IDENTIFIER_PREFIX)?)
            })
            .collect())
    }

    /// Mark the nameplate as in use until the code expires.
    pub(crate) async fn open(
        &self,
        nameplate: Nameplate,
        now: u64,
        expires_at: u64,
    ) -> Result<Open, RendezvousError> {
        let event = self
            .identity
            .sign_event(
                JOIN_EVENT_KIND,
                tags(open_identifier(nameplate), expires_at),
                String::new(),
                now,
            )
            .map_err(|_| RendezvousError::Identity)?;
        self.publish(&event.json).await?;
        Ok(Open {
            id: event.id,
            owned_device: event.author,
            created_at: now,
        })
    }

    /// The newest open event at this nameplate that has not expired.
    pub(crate) async fn find_open(
        &self,
        nameplate: Nameplate,
        now: u64,
        lifetime: u64,
    ) -> Result<Option<Open>, RendezvousError> {
        let filter = RelayFilter {
            identifiers: vec![open_identifier(nameplate)],
            ..self.filter(None, now.saturating_sub(lifetime + CLOCK_SKEW_SECONDS))
        };
        let own = self.identity.device_public_key().unwrap_or_default();
        Ok(self
            .read(&filter)
            .await?
            .iter()
            .filter_map(|event_json| DeviceIdentity::verify_event(event_json).ok())
            .filter(|event| {
                event.kind == JOIN_EVENT_KIND
                    && event.content.is_empty()
                    && event.author != own
                    && event.tags.first().map(Vec::as_slice)
                        == Some(&["d".to_owned(), open_identifier(nameplate)][..])
                    && is_current(&event.tags, now)
            })
            .max_by(|left, right| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| right.id.cmp(&left.id))
            })
            .map(|event| Open {
                id: event.id,
                owned_device: event.author,
                created_at: event.created_at,
            }))
    }

    /// Send one message to the other device. Returns its event id.
    pub(crate) async fn send(
        &self,
        nameplate: Nameplate,
        peer: &str,
        message: &Message,
        now: u64,
        expires_at: u64,
    ) -> Result<String, RendezvousError> {
        let plaintext = serde_json::to_string(message).map_err(|_| RendezvousError::Identity)?;
        let event = self
            .identity
            .encrypt_tagged_event(
                peer,
                JOIN_EVENT_KIND,
                tags(message_identifier(nameplate, message.step()), expires_at),
                &plaintext,
                now,
            )
            .map_err(|_| RendezvousError::Identity)?;
        self.publish(&event.json).await?;
        Ok(event.id)
    }

    /// Messages of these steps, from one device or from any, sent since a
    /// time, oldest first. Messages this device cannot open are skipped.
    pub(crate) async fn receive(
        &self,
        nameplate: Nameplate,
        peer: Option<&str>,
        steps: &[Step],
        since: u64,
        now: u64,
    ) -> Result<Vec<Received>, RendezvousError> {
        let filter = RelayFilter {
            identifiers: steps
                .iter()
                .map(|step| message_identifier(nameplate, *step))
                .collect(),
            ..self.filter(peer, since.saturating_sub(CLOCK_SKEW_SECONDS))
        };
        let mut seen = BTreeSet::new();
        let mut messages: Vec<(u64, String, String, Message)> = self
            .read(&filter)
            .await?
            .iter()
            .filter_map(|event_json| {
                let event = DeviceIdentity::verify_event(event_json).ok()?;
                if !seen.insert(event.id.clone())
                    || event.kind != JOIN_EVENT_KIND
                    || peer.is_some_and(|peer| peer != event.author)
                    || !is_current(&event.tags, now)
                {
                    return None;
                }
                let opened = self
                    .identity
                    .decrypt_event_from(event_json, &event.author)
                    .ok()?;
                let message: Message = serde_json::from_str(&opened.content).ok()?;
                let identifier = message_identifier(nameplate, message.step());
                (steps.contains(&message.step())
                    && event.tags.first().map(Vec::as_slice)
                        == Some(&["d".to_owned(), identifier][..]))
                .then_some((event.created_at, event.id, event.author, message))
            })
            .collect();
        // Oldest first, so the first message that fits wins.
        messages.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        Ok(messages
            .into_iter()
            .map(|(_, id, author, message)| Received {
                id,
                author,
                message,
            })
            .collect())
    }

    /// Post an event that is not a join message, such as the vouch itself.
    pub(crate) async fn publish(&self, event_json: &str) -> Result<(), RendezvousError> {
        let auth: Arc<dyn RelayAuthSigner> = Arc::new(DeviceAuthSigner(self.identity.clone()));
        let results = futures::future::join_all(
            self.relays
                .as_slice()
                .iter()
                .map(|relay| self.transport.publish(relay, event_json, auth.clone())),
        )
        .await;
        if results.iter().any(Result::is_ok) {
            Ok(())
        } else {
            Err(RendezvousError::Publish)
        }
    }

    async fn read(&self, filter: &RelayFilter) -> Result<Vec<String>, RendezvousError> {
        let auth: Arc<dyn RelayAuthSigner> = Arc::new(DeviceAuthSigner(self.identity.clone()));
        let results = futures::future::join_all(
            self.relays
                .as_slice()
                .iter()
                .map(|relay| self.transport.read(relay, filter, auth.clone())),
        )
        .await;
        if results.iter().all(Result::is_err) {
            return Err(RendezvousError::Read);
        }
        Ok(results
            .into_iter()
            .flatten()
            .flatten()
            .map(|event| event.event_json)
            .collect())
    }

    fn filter(&self, author: Option<&str>, since: u64) -> RelayFilter {
        RelayFilter {
            kinds: vec![JOIN_EVENT_KIND],
            recipient_public_key: None,
            author_public_key: author.map(str::to_owned),
            since: Some(since),
            limit: MAX_READ_EVENTS,
            identifiers: Vec::new(),
        }
    }
}

fn tags(identifier: String, expires_at: u64) -> Vec<Vec<String>> {
    vec![
        vec!["d".into(), identifier],
        vec!["expiration".into(), expires_at.to_string()],
    ]
}

/// Join events always carry an expiration; one that has passed is ignored
/// even when a relay still holds it.
fn is_current(tags: &[Vec<String>], now: u64) -> bool {
    tags.iter()
        .find(|tag| tag.len() == 2 && tag[0] == "expiration")
        .and_then(|tag| tag[1].parse::<u64>().ok())
        .is_some_and(|expires_at| expires_at > now)
}
