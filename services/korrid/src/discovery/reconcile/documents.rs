//! Conflict gates and forward recovery for the three fixed readable documents.
//! The existing private repair journal records a write before any rename. A
//! restart may finish it only while every file is still old or already new.
use super::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct Documents {
    pub device: String,
    pub games: String,
    pub releases: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct PendingWrite {
    expected: Documents,
    candidate: Documents,
}

impl Documents {
    pub fn read(root: &Path) -> Result<Self, DiscoveryError> {
        Ok(Self {
            device: read_fixed(root, DEVICE_FILE_NAME)?,
            games: read_fixed(root, GAMES_FILE_NAME)?,
            releases: read_fixed(root, RELEASES_FILE_NAME)?,
        })
    }

    pub fn validate(&self) -> Result<config::ConfigSnapshot, DiscoveryError> {
        config::decode_config_documents(&self.device, &self.games, &self.releases)
            .map_err(|error| DiscoveryError::Candidate(error.to_string()))
    }

    fn files(&self) -> [(&str, &str); 3] {
        // Locations publish last. A partial catalog fails link validation and
        // the snapshot coordinator retains the last known good generation.
        [
            (GAMES_FILE_NAME, &self.games),
            (RELEASES_FILE_NAME, &self.releases),
            (DEVICE_FILE_NAME, &self.device),
        ]
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
        if self == &candidate {
            return Ok(());
        }
        private.repair.pending_write = Some(PendingWrite {
            expected: self.clone(),
            candidate,
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
    #[cfg(test)]
    tests::publication_boundary(0, root, private_root)?;
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
        #[cfg(test)]
        tests::publication_boundary(
            pending
                .candidate
                .files()
                .iter()
                .position(|(file, _)| *file == name)
                .unwrap()
                + 1,
            root,
            private_root,
        )?;
    }
    // An editor can change an earlier file while later files publish. Keep
    // both the repair plan and ownership pending unless all bytes still agree.
    if Documents::read(root)? != pending.candidate {
        return Err(DiscoveryError::Conflict);
    }
    apply_pending_ownership(&pending.candidate, private)?;
    private.repair.pending_write = None;
    private.write(private_root)?;
    Ok(true)
}

#[cfg(test)]
mod tests;
