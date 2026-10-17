//! Conflict gates and forward recovery for the two fixed readable documents.
//! The existing private repair journal records a write before any rename. A
//! restart may finish it only while every file is still old or already new.
use super::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct Documents {
    pub device: String,
    pub library: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct PendingWrite {
    expected: Documents,
    candidate: Documents,
    ownership: OwnershipJournal,
}

impl Documents {
    pub fn read(root: &Path) -> Result<Self, DiscoveryError> {
        Ok(Self {
            device: read_fixed(root, DEVICE_FILE_NAME)?,
            library: read_fixed(root, LIBRARY_FILE_NAME)?,
        })
    }

    pub fn validate(&self) -> Result<config::ConfigSnapshot, DiscoveryError> {
        config::decode_config_documents(&self.device, &self.library)
            .map_err(|error| DiscoveryError::Candidate(error.to_string()))
    }

    fn files(&self) -> [(&str, &str); 2] {
        // Locations publish last. A partial catalog fails link validation and
        // the snapshot coordinator retains the last known good generation.
        [
            (LIBRARY_FILE_NAME, &self.library),
            (DEVICE_FILE_NAME, &self.device),
        ]
    }

    pub fn with_snapshot(
        &self,
        before: &ConfigSnapshot,
        after: &ConfigSnapshot,
    ) -> Result<Self, DiscoveryError> {
        let mut device = parse_mapping(&self.device)?;
        let old = change::device_value(before);
        let new = change::device_value(after);
        for key in old.keys().chain(new.keys()) {
            if old.get(key) != new.get(key) {
                if let Some(value) = new.get(key) {
                    device.insert(key.clone(), value.clone());
                } else {
                    device.remove(key);
                }
            }
        }
        let device = if parse_mapping(&self.device)? == device {
            self.device.clone()
        } else {
            serialize_mapping(device)?
        };
        let library = if before.library == after.library {
            self.library.clone()
        } else {
            let mut document = parse_mapping(&self.library)?;
            document.insert(
                "library".into(),
                serde_yaml::to_value(&after.library)
                    .map_err(|e| DiscoveryError::Candidate(e.to_string()))?,
            );
            serialize_mapping(document)?
        };
        Ok(Self { device, library })
    }

    pub fn commit(
        &self,
        candidate: Self,
        root: &Path,
        private_root: &Path,
        private: &mut PrivateState,
    ) -> Result<(), DiscoveryError> {
        candidate.validate()?;
        if &Self::read(root)? != self {
            return Err(DiscoveryError::Conflict);
        }
        let ownership = private
            .next_ownership
            .take()
            .unwrap_or_else(|| private.ownership.clone());
        if self == &candidate && ownership == private.ownership {
            return Ok(());
        }
        private.repair.pending_write = Some(PendingWrite {
            expected: self.clone(),
            candidate,
            ownership,
        });
        private.write(private_root)?;
        recover(root, private_root, private)?;
        Ok(())
    }
}

pub(super) fn recover(
    root: &Path,
    private_root: &Path,
    private: &mut PrivateState,
) -> Result<bool, DiscoveryError> {
    let Some(pending) = private.repair.pending_write.clone() else {
        return Ok(false);
    };
    let current = Documents::read(root)?;
    for (((_, old), (_, new)), (_, now)) in pending
        .expected
        .files()
        .into_iter()
        .zip(pending.candidate.files())
        .zip(current.files())
    {
        if now != old && now != new {
            return Err(DiscoveryError::Conflict);
        }
    }
    pending.candidate.validate()?;

    for (((name, old), (_, new)), (_, now)) in pending
        .expected
        .files()
        .into_iter()
        .zip(pending.candidate.files())
        .zip(current.files())
    {
        if now != new {
            write_atomically(&root.join(name), new.as_bytes(), &revision(old))?;
        }
    }
    // An editor can change an earlier file while later files publish. Keep
    // both the repair plan and ownership pending unless all bytes still agree.
    if Documents::read(root)? != pending.candidate {
        return Err(DiscoveryError::Conflict);
    }
    private.ownership = pending.ownership;
    private.repair.pending_write = None;
    private.write(private_root)?;
    Ok(true)
}

#[cfg(test)]
mod tests;
