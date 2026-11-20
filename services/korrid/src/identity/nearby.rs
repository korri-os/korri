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

    /// ADR 0089 local discovery: the announcement tag for this device and
    /// `peer_public_key` in one five-minute `window`. Tag =
    /// HMAC-SHA256(key = secp256k1 ECDH shared secret (x coordinate),
    /// msg = "korri-nearby-v1" || window as 8 big-endian bytes), first 8 bytes.
    /// ECDH is symmetric, so both devices of the pair derive the same tag and
    /// no other key can. The shared secret never leaves identity.
    pub fn nearby_announcement_tag(
        &self,
        peer_public_key: &str,
        window: u64,
    ) -> Result<[u8; 8], IdentityError> {
        let peer = parse_public_key(peer_public_key)?;
        let keys = self.keys.as_ref().ok_or(IdentityError::NoDeviceKey)?;
        if keys.public_key() == peer {
            return Err(IdentityError::Encryption);
        }
        // A Nostr key is x-only; either lift of x gives the same shared x.
        let mut compressed = [0u8; 33];
        compressed[0] = 0x02;
        compressed[1..].copy_from_slice(peer.as_bytes());
        let point =
            secp256k1::PublicKey::from_slice(&compressed).map_err(|_| IdentityError::Encryption)?;
        let shared = Zeroizing::new(secp256k1::ecdh::shared_secret_point(
            &point,
            keys.secret_key(),
        ));
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&shared[..32]).map_err(|_| IdentityError::Encryption)?;
        mac.update(b"korri-nearby-v1");
        mac.update(&window.to_be_bytes());
        let mut tag = [0u8; 8];
        tag.copy_from_slice(&mac.finalize().into_bytes()[..8]);
        Ok(tag)
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
    fn nearby_announcement_tag_is_pairwise_and_rotates_with_the_window() {
        let roots: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
        let identities: Vec<_> = roots
            .iter()
            .map(|root| DeviceIdentity::load_or_create(root.path()).unwrap())
            .collect();
        let [a, b, c] = identities.as_slice() else {
            unreachable!()
        };
        let key = |identity: &DeviceIdentity| identity.device_public_key().unwrap().to_owned();
        let ab = a.nearby_announcement_tag(&key(b), 7).unwrap();
        // Both members of the pair derive the same tag.
        assert_eq!(ab, b.nearby_announcement_tag(&key(a), 7).unwrap());
        // A key outside the pair cannot derive it for either member.
        assert_ne!(ab, c.nearby_announcement_tag(&key(a), 7).unwrap());
        assert_ne!(ab, c.nearby_announcement_tag(&key(b), 7).unwrap());
        // A window change changes the tag.
        assert_ne!(ab, a.nearby_announcement_tag(&key(b), 8).unwrap());
        assert!(a.nearby_announcement_tag(&key(a), 7).is_err());
        assert!(a.nearby_announcement_tag("not a key", 7).is_err());
    }

    /// The tag is exactly HMAC-SHA256(ECDH x, "korri-nearby-v1" || window
    /// big-endian), truncated to 8 bytes, so another implementation can match it.
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
        assert_eq!(
            identity
                .nearby_announcement_tag(&b.public_key().to_hex(), 5_900_000)
                .unwrap(),
            mac.finalize().into_bytes()[..8]
        );
    }
}
