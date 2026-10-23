//! Process-local signals. Dismissal names one device and one exact launch.
use super::Invite;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct Cache {
    invites: HashMap<(String, String), Invite>,
    dismissed: HashSet<(String, String)>,
}

impl Cache {
    pub(crate) fn replace(&mut self, sessions: Vec<Invite>) {
        // Absence can be a failed peer observation, not an observed End.
        // Retire a dismissal only when this device announces another launch.
        self.dismissed.retain(|(device, launch)| {
            !sessions.iter().any(|invite| {
                &invite.device_public_key == device && &invite.session.launch_id != launch
            })
        });
        let mut next = HashMap::new();
        for mut invite in sessions {
            let key = key(&invite);
            if self.dismissed.contains(&key) {
                continue;
            }
            invite.invite_id = self
                .invites
                .get(&key)
                .map(|old| old.invite_id.clone())
                .unwrap_or_else(|| format!("{:032x}", rand::random::<u128>()));
            next.insert(key, invite);
        }
        self.invites = next;
    }

    pub(crate) fn clear(&mut self) {
        self.invites.clear();
    }

    pub(crate) fn list(&self) -> Vec<Invite> {
        let mut invites: Vec<_> = self.invites.values().cloned().collect();
        invites.sort_by(|a, b| a.device_public_key.cmp(&b.device_public_key));
        invites
    }

    pub(crate) fn get(&self, id: &str) -> Option<Invite> {
        self.invites
            .values()
            .find(|invite| invite.invite_id == id)
            .cloned()
    }

    pub(crate) fn dismiss(&mut self, id: &str) -> bool {
        let Some(invite) = self.get(id) else {
            return false;
        };
        let key = key(&invite);
        self.invites.remove(&key);
        self.dismissed.insert(key);
        true
    }
}

fn key(invite: &Invite) -> (String, String) {
    (
        invite.device_public_key.clone(),
        invite.session.launch_id.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invite(device: &str, launch: &str) -> Invite {
        serde_json::from_value(serde_json::json!({
            "inviteId":"", "devicePublicKey":device, "deviceLabel":"Device", "personName":"Person",
            "session":{"launchId":launch, "phase":"running"}
        }))
        .unwrap()
    }

    #[test]
    fn stable_ids_dismiss_one_launch_and_end_removes_the_invite() {
        let mut cache = Cache::default();
        cache.replace(vec![invite("a", "1"), invite("b", "1")]);
        let first = cache.list()[0].invite_id.clone();
        cache.replace(vec![invite("a", "1"), invite("b", "1")]);
        assert_eq!(cache.list()[0].invite_id, first);
        assert!(cache.dismiss(&first));
        cache.replace(vec![invite("a", "1"), invite("b", "1")]);
        assert_eq!(cache.list().len(), 1);
        cache.replace(vec![]); // Missing a peer is not proof that its launch ended.
        cache.replace(vec![invite("a", "1")]);
        assert!(cache.list().is_empty());
        cache.clear(); // Device is not idle. Dismissal must survive this.
        cache.replace(vec![invite("a", "1")]);
        assert!(cache.list().is_empty());
        cache.replace(vec![invite("a", "2")]);
        assert_eq!(cache.list().len(), 1);
        assert_ne!(cache.list()[0].invite_id, first);
        cache.replace(vec![]);
        assert!(cache.list().is_empty());
    }
}
