//! Independent transport identity. No conversion from the Nostr device key.
use super::*;

/// Follow identity.rs::load_or_create_named_keys: the same storage lock,
/// private directory, bounded read, create_new 0600 write, and directory fsync.
/// DeviceIdentity::reset removes this file with the other keys.
pub(crate) fn load_or_create(root: &Path) -> Result<::iroh::SecretKey, IdentityError> {
    let _storage = IDENTITY_STORAGE
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| IdentityError::Storage)?;
    let directory = prepare_identity_directory(root)?;
    let path = directory.join("iroh.key");
    let bytes = match read_bounded_optional(&path, 65)? {
        Some(bytes) => bytes,
        None => {
            let key = ::iroh::SecretKey::generate();
            let encoded = Zeroizing::new(format!("{}\n", hex::encode(key.to_bytes())));
            if write_new_private(&path, encoded.as_bytes())? {
                return Ok(key);
            }
            read_bounded_optional(&path, 65)?.ok_or(IdentityError::Storage)?
        }
    };
    let bytes = Zeroizing::new(bytes);
    let text = std::str::from_utf8(&bytes).map_err(|_| IdentityError::Storage)?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.len() != 64 || !text.bytes().all(is_lower_hex) {
        return Err(IdentityError::Storage);
    }
    let mut secret = Zeroizing::new([0u8; 32]);
    hex::decode_to_slice(text, &mut *secret).map_err(|_| IdentityError::Storage)?;
    Ok(::iroh::SecretKey::from_bytes(&secret))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reach_key_is_private_stable_and_rejects_corruption() {
        let root = tempfile::tempdir().unwrap();
        let a = load_or_create(root.path()).unwrap();
        assert_eq!(a.public(), load_or_create(root.path()).unwrap().public());
        let path = root.path().join("identity/iroh.key");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::write(&path, "broken").unwrap();
        assert!(load_or_create(root.path()).is_err());
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(root.path().join("identity/device.key"), &path).unwrap();
        assert!(load_or_create(root.path()).is_err());
    }
}
