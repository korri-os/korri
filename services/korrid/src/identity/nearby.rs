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
}
