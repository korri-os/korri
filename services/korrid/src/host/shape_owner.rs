//! Connection order for one exact streamed launch. Callers supply identities
//! from authentication, never a device name supplied by a request.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Connection(u64);

#[derive(Default)]
pub(super) struct ShapeOwners<K> {
    connections: Vec<(K, Connection)>,
    next: u64,
}

impl<K: Eq> ShapeOwners<K> {
    /// Called for a new confirmed connection, not a heartbeat or size report.
    /// A reconnect joins at the back, behind viewers that stayed connected.
    pub(super) fn connect(&mut self, device: K) -> Connection {
        self.next = self
            .next
            .checked_add(1)
            .expect("connection generation exhausted");
        let connection = Connection(self.next);
        self.connections.retain(|(existing, _)| *existing != device);
        self.connections.push((device, connection));
        connection
    }

    pub(super) fn disconnect(&mut self, device: &K, connection: Connection) -> bool {
        let Some(index) = self
            .connections
            .iter()
            .position(|(key, token)| key == device && *token == connection)
        else {
            return false;
        };
        self.connections.remove(index);
        true
    }

    pub(super) fn owner(&self) -> Option<&K> {
        self.connections.first().map(|(device, _)| device)
    }

    pub(super) fn is_owner(&self, device: &K, connection: Connection) -> bool {
        self.connections
            .first()
            .is_some_and(|(key, token)| key == device && *token == connection)
    }
}

#[cfg(test)]
#[path = "shape_owner_tests.rs"]
mod tests;
