#![no_std]
#![forbid(unsafe_code)]
#![allow(missing_docs)]

extern crate alloc;

mod encoding;
use core::marker::PhantomData;

use subtle::ConstantTimeEq;

pub use encoding::SessionAD;
mod errors;
mod formatting;
mod keys;

use alloc::{borrow::ToOwned, vec::Vec};
use curve25519_dalek::{ristretto::CompressedRistretto, scalar::Scalar};
pub use encoding::NetworkEncoder;
pub use encoding::NoAD;
pub use errors::{Error, InvalidAuthTag, RandomError};
pub use formatting::hasher_lv_cat;
use formatting::{lv_cat, Hasher};
use getrandom::getrandom;
use hmac_sha512::HMAC;
use keys::{calculate_generator, compute_isk, compute_shared_k, derive_verifying_key};

pub const SESSION_ID_BYTES: usize = 16;
pub type SessionID = [u8; SESSION_ID_BYTES];

pub const SHARED_KEY_BYTES: usize = 64;
pub type SharedKey = [u8; SHARED_KEY_BYTES];

pub const BIG_Y_BYTES: usize = 32;
pub type Packet = [u8; BIG_Y_BYTES];

pub struct ChannelIdentifier<'a> {
    pub id_a: &'a [u8],
    pub id_b: &'a [u8],
    pub extra_context: &'a [&'a [u8]],
}

impl<'a> ChannelIdentifier<'a> {
    #[cfg(not(tarpaulin_include))] // Used in a `const`, so coverage can't be detected.
    pub const fn identities_only(id_a: &'a [u8], id_b: &'a [u8]) -> ChannelIdentifier<'a> {
        ChannelIdentifier {
            id_a,
            id_b,
            extra_context: &[],
        }
    }
}

impl ChannelIdentifier<'_> {
    fn mixed(&self) -> [u8; 64] {
        let mut f = Hasher::new();

        lv_cat(&mut f, &[self.id_a, self.id_b]);
        lv_cat(&mut f, self.extra_context);

        f.summarize()
    }
}

struct Transcript<'a> {
    msg_a: &'a [u8],
    msg_b: &'a [u8],
}

#[derive(Debug, Clone)]
pub struct CPace {
    session_id: SessionID,
    private_y: Scalar,
    msg: Vec<u8>,
}

impl CPace {
    pub fn new<T: NetworkEncoder>(
        session_id: &SessionID,
        prs: &[u8],
        channel_identifier: &ChannelIdentifier,
        ad: &T,
    ) -> Result<Self, RandomError> {
        let ci = channel_identifier.mixed();

        let g = calculate_generator(prs, &ci, session_id);

        let mut r = [0u8; 64];
        getrandom(&mut r).map_err(RandomError)?;
        let private_y = Scalar::from_bytes_mod_order_wide(&r);

        let big_y = g * private_y;

        let big_y = big_y.compress().to_bytes();

        let msg = ad.network_encode_with(&big_y);

        Ok(CPace {
            session_id: session_id.to_owned(),
            msg,
            private_y,
        })
    }

    pub fn message(&self) -> &[u8] {
        &self.msg
    }

    pub fn initiator_process_message<T: NetworkEncoder>(
        self,
        other_msg: Vec<u8>,
    ) -> Result<(UnverifiedNegotiation, T), Error> {
        let (ad, other_big_y) = T::network_decode_from(&other_msg)?;

        let isk = self.compute_key(
            other_big_y,
            Side::Initiator.transcript(&self.msg, &other_msg),
        )?;

        Ok((
            UnverifiedNegotiation {
                isk,
                own_msg: self.msg,
                other_msg,
            },
            ad,
        ))
    }

    pub fn responder_process_message<T: NetworkEncoder>(
        other_msg: Vec<u8>,
    ) -> Result<(ProcessedMessage<T>, T), Error> {
        let (ad, other_big_y) = T::network_decode_from(&other_msg)?;

        Ok((
            ProcessedMessage {
                other_msg,
                other_big_y,
                _ad: PhantomData,
            },
            ad,
        ))
    }

    fn compute_key(
        &self,
        other_big_y: [u8; 32],
        transcript: Transcript,
    ) -> Result<SharedKey, Error> {
        let other_big_y = CompressedRistretto(other_big_y)
            .decompress()
            .ok_or(Error::InvalidPublicKey)?;

        let k = compute_shared_k(self.private_y, &other_big_y)?;

        let isk = compute_isk(k, &self.session_id, transcript);

        Ok(isk)
    }
}

pub struct ProcessedMessage<T: NetworkEncoder> {
    other_msg: Vec<u8>,
    other_big_y: Packet,
    _ad: PhantomData<T>,
}

impl<T: NetworkEncoder> ProcessedMessage<T> {
    pub fn finish(self, ctx: CPace) -> Result<UnverifiedNegotiation, Error> {
        let isk = ctx.compute_key(
            self.other_big_y,
            Side::Responder.transcript(&ctx.msg, &self.other_msg),
        )?;

        Ok(UnverifiedNegotiation {
            isk,
            own_msg: ctx.msg,
            other_msg: self.other_msg,
        })
    }
}

enum Side {
    Initiator,
    Responder,
}

impl Side {
    fn transcript<'a>(self, own_msg: &'a [u8], other_msg: &'a [u8]) -> Transcript<'a> {
        match self {
            Side::Initiator => Transcript {
                msg_a: own_msg,
                msg_b: other_msg,
            },
            Side::Responder => Transcript {
                msg_a: other_msg,
                msg_b: own_msg,
            },
        }
    }
}

#[cfg_attr(test, derive(Clone, Debug))]
pub struct UnverifiedNegotiation {
    pub isk: SharedKey,
    own_msg: Vec<u8>,
    other_msg: Vec<u8>,
}

impl UnverifiedNegotiation {
    pub fn tag(&self) -> [u8; 64] {
        let verifying_key = derive_verifying_key(&self.isk);

        HMAC::mac(&self.own_msg, verifying_key)
    }

    pub fn own_message(&self) -> &[u8] {
        &self.own_msg
    }

    pub fn other_message(&self) -> &[u8] {
        &self.other_msg
    }

    pub fn verify_tag(self, other_auth_tag: &[u8]) -> Result<SharedKey, InvalidAuthTag> {
        let verifying_key = derive_verifying_key(&self.isk);

        let expected_tag = HMAC::mac(&self.other_msg, verifying_key);

        match bool::from(other_auth_tag.ct_eq(&expected_tag)) {
            true => Ok(self.isk),
            false => Err(InvalidAuthTag(())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use curve25519_dalek::traits::Identity;

    const CI: ChannelIdentifier = ChannelIdentifier::identities_only(b"client", b"server");

    fn setup_client_a() -> CPace {
        let mut session_id = [0u8; SESSION_ID_BYTES];
        getrandom(&mut session_id).unwrap();

        CPace::new(&session_id, b"password", &CI, &SessionAD { session_id }).unwrap()
    }

    fn setup_client_b(msg_a: Vec<u8>) -> Result<UnverifiedNegotiation, Error> {
        let (processed_msg, ad) = CPace::responder_process_message::<SessionAD>(msg_a)?;

        let ctx = CPace::new(&ad.session_id, b"password", &CI, &NoAD {}).unwrap();
        processed_msg.finish(ctx)
    }

    fn setup_session() -> Result<(UnverifiedNegotiation, UnverifiedNegotiation), Error> {
        let client_a = setup_client_a();
        let msg_a = client_a.message().to_owned();

        let client_b = setup_client_b(msg_a)?;
        let msg_b = client_b.own_message().to_owned();

        let (client_a, _) = client_a.initiator_process_message::<NoAD>(msg_b)?;

        Ok((client_a, client_b))
    }

    #[test]
    fn test_cpace() {
        let (client_a, client_b) = setup_session().unwrap();

        assert_eq!(client_a.own_message(), client_b.other_message());
        assert_eq!(client_a.other_message(), client_b.own_message());

        let (auth_a, auth_b) = (client_a.tag(), client_b.tag());

        let client_a_key = client_a.verify_tag(&auth_b).unwrap();
        let client_b_key = client_b.verify_tag(&auth_a).unwrap();

        assert_eq!(client_a_key, client_b_key);
    }

    #[test]
    fn invalid_tags_are_rejected() {
        let (client_a, client_b) = setup_session().unwrap();

        let (real_auth_a, real_auth_b) = (client_a.tag(), client_b.tag());
        // Simulate an attacker tampering with A's tag over the network:
        let mut malicious_auth_a = real_auth_a;
        malicious_auth_a[32] = real_auth_a[32].wrapping_add(1);

        assert!(client_b.clone().verify_tag(&malicious_auth_a).is_err());
        assert!(client_a.clone().verify_tag(&real_auth_b).is_ok());

        // Simulate an attacker tampering with B's tag over the network:
        let mut malicious_auth_b = real_auth_b;
        malicious_auth_b[32] = real_auth_b[32].wrapping_add(2);

        assert!(client_a.verify_tag(&malicious_auth_b).is_err());
        assert!(client_b.verify_tag(&real_auth_a).is_ok());
    }

    #[test]
    fn invalid_public_values_rejected() {
        let client_a = setup_client_a();
        let mut malicious_msg_a = client_a.message().to_owned();

        // Use an obviously wrong value to ensure we use the correct decompression routine
        // for both the initiator and responder.
        let evil_public = curve25519_dalek::constants::ED25519_BASEPOINT_POINT
            .compress()
            .to_bytes();

        malicious_msg_a[SESSION_ID_BYTES..].copy_from_slice(&evil_public);
        assert!(matches!(
            setup_client_b(malicious_msg_a),
            Err(Error::InvalidPublicKey)
        ));

        let client_b = setup_client_b(client_a.message().to_owned()).unwrap();

        let mut malicious_msg_b = client_b.own_message().to_vec();
        malicious_msg_b.copy_from_slice(&evil_public);
        assert!(matches!(
            client_a
                .clone()
                .initiator_process_message::<NoAD>(malicious_msg_b),
            Err(Error::InvalidPublicKey)
        ));

        // Ensure that we verify `k` in both directions when using the public API. This is also tested at a lower level
        // in `./src/keys.rs`.
        let evil_public = CompressedRistretto::identity().to_bytes();

        let mut malicious_msg_a = client_a.message().to_owned();
        malicious_msg_a[SESSION_ID_BYTES..].copy_from_slice(&evil_public);

        assert!(matches!(
            setup_client_b(malicious_msg_a),
            Err(Error::InvalidSharedKey)
        ));

        let malicious_msg_b = evil_public;
        assert!(matches!(
            client_a.initiator_process_message::<NoAD>(malicious_msg_b.to_vec()),
            Err(Error::InvalidSharedKey)
        ));
    }
}
