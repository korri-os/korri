//! Nearby-only key material. The NIP-44 conversation key stays inside identity.
use super::*;
use hmac::{Hmac, Mac};
use sha2::Sha256;

impl DeviceIdentity {
    pub fn nearby_key_material(
        &self,
        peer_public_key: &str,
    ) -> Result<Zeroizing<[u8; 32]>, IdentityError> {
        let peer = parse_public_key(peer_public_key)?;
        let keys = self.keys.as_ref().ok_or(IdentityError::NoDeviceKey)?;
        let local = keys.public_key().to_hex();
        if local == peer_public_key {
            return Err(IdentityError::Encryption);
        }
        // nip44::encrypt and nip44::decrypt use this key agreement in identity.rs.
        let conversation =
            nostr::nips::nip44::v2::ConversationKey::derive(keys.secret_key(), &peer)
                .map_err(|_| IdentityError::Encryption)?;
        let mut mac = Hmac::<Sha256>::new_from_slice(conversation.as_bytes())
            .map_err(|_| IdentityError::Encryption)?;
        mac.update(b"korri/nearby/key-material/v1\0");
        let (lower, upper) = if local.as_str() < peer_public_key {
            (local.as_str(), peer_public_key)
        } else {
            (peer_public_key, local.as_str())
        };
        mac.update(lower.as_bytes());
        mac.update(upper.as_bytes());
        Ok(Zeroizing::new(mac.finalize().into_bytes().into()))
    }

    /// ADR 0089 local discovery: this device's pair with `peer_public_key`,
    /// holding their secp256k1 ECDH shared secret (x coordinate). One ECDH
    /// per known device; callers keep the pair while the device set stays
    /// the same, and derive each window's tags from it.
    pub fn nearby_pair(&self, peer_public_key: &str) -> Result<NearbyPair, IdentityError> {
        let peer = parse_public_key(peer_public_key)?;
        let keys = self.keys.as_ref().ok_or(IdentityError::NoDeviceKey)?;
        let local = keys.public_key();
        if local == peer {
            return Err(IdentityError::Encryption);
        }
        // A Nostr key is x-only; either lift of x gives the same shared x.
        let mut compressed = [0u8; 33];
        compressed[0] = 0x02;
        compressed[1..].copy_from_slice(peer.as_bytes());
        let point =
            secp256k1::PublicKey::from_slice(&compressed).map_err(|_| IdentityError::Encryption)?;
        let point = Zeroizing::new(secp256k1::ecdh::shared_secret_point(
            &point,
            keys.secret_key(),
        ));
        let mut shared = Zeroizing::new([0u8; 32]);
        shared.copy_from_slice(&point[..32]);
        Ok(NearbyPair {
            shared,
            local: local.to_bytes(),
            peer: peer.to_bytes(),
        })
    }
}

/// One device pair's local-discovery key. It holds the pair's shared secret,
/// which never leaves this type: it has no accessor and no `Debug`, and it
/// is erased on drop.
pub struct NearbyPair {
    shared: Zeroizing<[u8; 32]>,
    local: [u8; 32],
    peer: [u8; 32],
}
impl NearbyPair {
    /// The tag this device announces to the peer in `window`.
    pub fn tag_to_peer(&self, window: u64) -> [u8; 8] {
        self.tag(window, &self.local)
    }
    /// The tag the peer announces to this device in `window`, which a
    /// listener looks for.
    pub fn tag_from_peer(&self, window: u64) -> [u8; 8] {
        self.tag(window, &self.peer)
    }
    /// Tag(sender S, receiver R) = HMAC-SHA256(key = secp256k1 ECDH(S, R)
    /// shared x coordinate, msg = "korri-nearby-v1" || window as 8
    /// big-endian bytes || S's 32-byte public key), first 8 bytes. Only S
    /// and R can derive it. S's key in the message makes the two directions
    /// differ, so A's and B's announcements share no tag.
    fn tag(&self, window: u64, sender: &[u8; 32]) -> [u8; 8] {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.shared.as_slice())
            .expect("HMAC accepts any key length");
        mac.update(b"korri-nearby-v1");
        mac.update(&window.to_be_bytes());
        mac.update(sender);
        let mut tag = [0u8; 8];
        tag.copy_from_slice(&mac.finalize().into_bytes()[..8]);
        tag
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearby_key_material_is_symmetric_stable_and_peer_specific() {
        let roots: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
        let identities: Vec<_> = roots
            .iter()
            .map(|root| DeviceIdentity::load_or_create(root.path()).unwrap())
            .collect();
        let [a, b, c] = identities.as_slice() else {
            unreachable!()
        };
        let ab = a
            .nearby_key_material(b.device_public_key().unwrap())
            .unwrap();
        let ba = b
            .nearby_key_material(a.device_public_key().unwrap())
            .unwrap();
        assert_eq!(*ab, *ba);
        assert_eq!(
            *ab,
            *a.nearby_key_material(b.device_public_key().unwrap())
                .unwrap()
        );
        assert_ne!(
            *ab,
            *a.nearby_key_material(c.device_public_key().unwrap())
                .unwrap()
        );
        assert!(a
            .nearby_key_material(a.device_public_key().unwrap())
            .is_err());
        assert!(a.nearby_key_material("not a key").is_err());
    }

    #[test]
    fn nearby_pair_tags_are_pairwise_directional_and_rotate_with_the_window() {
        let roots: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
        let identities: Vec<_> = roots
            .iter()
            .map(|root| DeviceIdentity::load_or_create(root.path()).unwrap())
            .collect();
        let [a, b, c] = identities.as_slice() else {
            unreachable!()
        };
        let pair = |from: &DeviceIdentity, to: &DeviceIdentity| {
            from.nearby_pair(to.device_public_key().unwrap()).unwrap()
        };
        let a_to_b = pair(a, b).tag_to_peer(7);
        // The receiver derives the tag the sender announces to it.
        assert_eq!(a_to_b, pair(b, a).tag_from_peer(7));
        // The other direction is a different tag.
        assert_ne!(a_to_b, pair(b, a).tag_to_peer(7));
        assert_ne!(a_to_b, pair(a, b).tag_from_peer(7));
        // A key outside the pair cannot derive it for either member.
        for other in [pair(c, a), pair(c, b)] {
            assert_ne!(a_to_b, other.tag_to_peer(7));
            assert_ne!(a_to_b, other.tag_from_peer(7));
        }
        // A window change changes the tag.
        assert_ne!(a_to_b, pair(a, b).tag_to_peer(8));
        assert!(a.nearby_pair(a.device_public_key().unwrap()).is_err());
        assert!(a.nearby_pair("not a key").is_err());
    }

    /// The tag is exactly HMAC-SHA256(ECDH x, "korri-nearby-v1" || window
    /// big-endian || sender key), truncated to 8 bytes, so another
    /// implementation can match it.
    #[test]
    fn nearby_announcement_tag_matches_its_written_derivation() {
        let a = Keys::parse("0000000000000000000000000000000000000000000000000000000000000003")
            .unwrap();
        let b = Keys::parse("0000000000000000000000000000000000000000000000000000000000000005")
            .unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut identity = DeviceIdentity::load_or_create(root.path()).unwrap();
        identity.keys = Some(a.clone());
        // x(3·5·G), independent of the Nostr encoding of either key.
        let shared = secp256k1::ecdh::shared_secret_point(
            &secp256k1::PublicKey::from_secret_key_global(
                &secp256k1::SecretKey::from_slice(&[[0u8; 31].as_slice(), &[15u8]].concat())
                    .unwrap(),
            ),
            &secp256k1::SecretKey::from_slice(&[[0u8; 31].as_slice(), &[1u8]].concat()).unwrap(),
        );
        let mut mac = Hmac::<Sha256>::new_from_slice(&shared[..32]).unwrap();
        mac.update(b"korri-nearby-v1");
        mac.update(&5_900_000u64.to_be_bytes());
        // The sender, A = 3·G, as its 32-byte x-only key.
        mac.update(a.public_key().as_bytes());
        assert_eq!(
            identity
                .nearby_pair(&b.public_key().to_hex())
                .unwrap()
                .tag_to_peer(5_900_000),
            mac.finalize().into_bytes()[..8]
        );
    }
}
