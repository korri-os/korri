//! The join's key exchange (ADR 0070): 1Password's audited CPace, copied
//! unchanged into vendor/pake-cpace and used only here.
//!
//! The owned device starts the exchange, as CPace's initiator. The two
//! device public keys are CPace's party identities. The version label and
//! the session nonce are its extra context, and the nonce is also CPace's
//! session id. A run therefore binds exactly these two devices, this
//! version and this session. Key confirmation is not optional: neither side
//! trusts the other until the other's confirmation checks out, and any
//! failure ends the run.

use pake_cpace::{CPace, ChannelIdentifier, NoAD, SessionAD, UnverifiedNegotiation};

/// The version label bound into every exchange.
pub(crate) const VERSION: &[u8] = b"korri-join-1";
pub(crate) const SESSION_BYTES: usize = pake_cpace::SESSION_ID_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum ExchangeError {
    /// A message or a key was not well formed.
    #[error("the key exchange message is malformed")]
    Malformed,
    /// The other side's confirmation does not match: a different code, a
    /// different device, or a changed message.
    #[error("the code did not match")]
    Mismatch,
    #[error("no random numbers are available")]
    Random,
}

/// The owned device's side after its first message.
pub(crate) struct Initiator {
    cpace: CPace,
    session: [u8; SESSION_BYTES],
}

impl Initiator {
    /// Start an exchange with a new session nonce. The returned message
    /// carries that nonce to the joining device.
    pub(crate) fn start(
        secret: &[u8],
        owned_device: &str,
        joining_device: &str,
    ) -> Result<(Self, Vec<u8>), ExchangeError> {
        let session: [u8; SESSION_BYTES] = rand::random();
        let (owned, joining) = (device_key(owned_device)?, device_key(joining_device)?);
        let context: [&[u8]; 2] = [VERSION, &session];
        let channel = ChannelIdentifier {
            id_a: &owned,
            id_b: &joining,
            extra_context: &context,
        };
        let cpace = CPace::new(
            &session,
            secret,
            &channel,
            &SessionAD {
                session_id: session,
            },
        )
        .map_err(|_| ExchangeError::Random)?;
        let message = cpace.message().to_vec();
        Ok((Self { cpace, session }, message))
    }

    pub(crate) fn session(&self) -> [u8; SESSION_BYTES] {
        self.session
    }

    /// Check the joining device's answer and its confirmation. On success,
    /// return this side's confirmation, for the joining device to check.
    pub(crate) fn finish(
        self,
        answer: &[u8],
        confirmation: &[u8],
    ) -> Result<Vec<u8>, ExchangeError> {
        let (negotiation, NoAD {}) = self
            .cpace
            .initiator_process_message::<NoAD>(answer.to_vec())
            .map_err(|_| ExchangeError::Malformed)?;
        let own = negotiation.tag().to_vec();
        negotiation
            .verify_tag(confirmation)
            .map_err(|_| ExchangeError::Mismatch)?;
        Ok(own)
    }
}

/// The joining device's side after its answer.
pub(crate) struct Responder {
    negotiation: UnverifiedNegotiation,
    session: [u8; SESSION_BYTES],
}

/// What the joining device sends back: its CPace message and its
/// confirmation.
pub(crate) struct Answer {
    pub message: Vec<u8>,
    pub confirmation: Vec<u8>,
}

impl Responder {
    /// Answer the owned device's first message.
    pub(crate) fn answer(
        secret: &[u8],
        owned_device: &str,
        joining_device: &str,
        offer: &[u8],
    ) -> Result<(Self, Answer), ExchangeError> {
        let (owned, joining) = (device_key(owned_device)?, device_key(joining_device)?);
        let (processed, ad) = CPace::responder_process_message::<SessionAD>(offer.to_vec())
            .map_err(|_| ExchangeError::Malformed)?;
        let context: [&[u8]; 2] = [VERSION, &ad.session_id];
        let channel = ChannelIdentifier {
            id_a: &owned,
            id_b: &joining,
            extra_context: &context,
        };
        let cpace = CPace::new(&ad.session_id, secret, &channel, &NoAD {})
            .map_err(|_| ExchangeError::Random)?;
        let negotiation = processed
            .finish(cpace)
            .map_err(|_| ExchangeError::Malformed)?;
        let answer = Answer {
            message: negotiation.own_message().to_vec(),
            confirmation: negotiation.tag().to_vec(),
        };
        Ok((
            Self {
                negotiation,
                session: ad.session_id,
            },
            answer,
        ))
    }

    pub(crate) fn session(&self) -> [u8; SESSION_BYTES] {
        self.session
    }

    /// Check the owned device's confirmation.
    pub(crate) fn confirm(self, confirmation: &[u8]) -> Result<(), ExchangeError> {
        self.negotiation
            .verify_tag(confirmation)
            .map(|_| ())
            .map_err(|_| ExchangeError::Mismatch)
    }
}

fn device_key(hex_key: &str) -> Result<[u8; 32], ExchangeError> {
    let mut key = [0; 32];
    hex::decode_to_slice(hex_key, &mut key).map_err(|_| ExchangeError::Malformed)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNED: &str = "aa00000000000000000000000000000000000000000000000000000000000001";
    const JOINING: &str = "bb00000000000000000000000000000000000000000000000000000000000002";
    const OTHER: &str = "cc00000000000000000000000000000000000000000000000000000000000003";

    fn run(
        owned_secret: &[u8],
        joining_secret: &[u8],
        joining_binds: (&str, &str),
    ) -> (Result<Vec<u8>, ExchangeError>, Option<Responder>) {
        let (initiator, offer) = Initiator::start(owned_secret, OWNED, JOINING).unwrap();
        let (responder, answer) =
            Responder::answer(joining_secret, joining_binds.0, joining_binds.1, &offer).unwrap();
        assert_eq!(responder.session(), initiator.session());
        (
            initiator.finish(&answer.message, &answer.confirmation),
            Some(responder),
        )
    }

    #[test]
    fn the_same_secret_and_devices_confirm_on_both_sides() {
        let (owned, responder) = run(b"4561234", b"4561234", (OWNED, JOINING));
        let confirmation = owned.unwrap();
        responder.unwrap().confirm(&confirmation).unwrap();
    }

    #[test]
    fn a_wrong_secret_fails_confirmation_on_both_sides() {
        let (initiator, offer) = Initiator::start(b"4561234", OWNED, JOINING).unwrap();
        let (responder, answer) = Responder::answer(b"4561235", OWNED, JOINING, &offer).unwrap();
        assert_eq!(
            initiator.finish(&answer.message, &answer.confirmation),
            Err(ExchangeError::Mismatch)
        );
        // The owned device sends nothing after a mismatch; any confirmation
        // the joining device sees is rejected.
        assert_eq!(responder.confirm(&[0; 64]), Err(ExchangeError::Mismatch));
    }

    #[test]
    fn a_different_device_key_fails_confirmation() {
        for binds in [(OTHER, JOINING), (OWNED, OTHER), (JOINING, OWNED)] {
            let (owned, _) = run(b"4561234", b"4561234", binds);
            assert_eq!(owned, Err(ExchangeError::Mismatch), "{binds:?}");
        }
    }

    #[test]
    fn a_changed_message_or_confirmation_fails() {
        let (initiator, offer) = Initiator::start(b"4561234", OWNED, JOINING).unwrap();
        let (_, answer) = Responder::answer(b"4561234", OWNED, JOINING, &offer).unwrap();
        let mut confirmation = answer.confirmation.clone();
        confirmation[0] ^= 1;
        assert_eq!(
            initiator.finish(&answer.message, &confirmation),
            Err(ExchangeError::Mismatch)
        );

        let (initiator, offer) = Initiator::start(b"4561234", OWNED, JOINING).unwrap();
        let (responder, answer) = Responder::answer(b"4561234", OWNED, JOINING, &offer).unwrap();
        let confirmation = initiator
            .finish(&answer.message, &answer.confirmation)
            .unwrap();
        let mut changed = confirmation.clone();
        changed[63] ^= 1;
        assert_eq!(responder.confirm(&changed), Err(ExchangeError::Mismatch));
    }

    #[test]
    fn a_confirmation_from_another_session_fails() {
        let (first, first_offer) = Initiator::start(b"4561234", OWNED, JOINING).unwrap();
        let (second, _) = Initiator::start(b"4561234", OWNED, JOINING).unwrap();
        assert_ne!(first.session(), second.session());
        let (_, answer) = Responder::answer(b"4561234", OWNED, JOINING, &first_offer).unwrap();
        assert_eq!(
            second.finish(&answer.message, &answer.confirmation),
            Err(ExchangeError::Mismatch)
        );
        drop(first);
    }

    #[test]
    fn malformed_messages_and_keys_are_refused() {
        assert_eq!(
            Responder::answer(b"4561234", OWNED, JOINING, &[1, 2, 3]).err(),
            Some(ExchangeError::Malformed)
        );
        assert_eq!(
            Initiator::start(b"4561234", "not hex", JOINING).err(),
            Some(ExchangeError::Malformed)
        );
        let (initiator, _) = Initiator::start(b"4561234", OWNED, JOINING).unwrap();
        assert_eq!(
            initiator.finish(&[0; 31], &[0; 64]),
            Err(ExchangeError::Malformed)
        );
    }
}
