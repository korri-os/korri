use crate::{Error, pipe::Transport};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PENDING: usize = 128;

pub(crate) enum Message<A> {
    Response { action: A, result: Value },
    Event(Value),
}

pub(crate) struct Pending<A> {
    commands: BTreeMap<u64, (String, Instant, A)>,
}
impl<A> Default for Pending<A> {
    fn default() -> Self {
        Self {
            commands: BTreeMap::new(),
        }
    }
}
impl<A> Pending<A> {
    pub(crate) fn insert(&mut self, id: u64, session: &str, action: A) -> Result<(), Error> {
        if self.commands.len() >= MAX_PENDING || self.commands.contains_key(&id) {
            return Err(Error::Protocol);
        }
        self.commands.insert(
            id,
            (session.into(), Instant::now() + COMMAND_TIMEOUT, action),
        );
        Ok(())
    }
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.commands
            .values()
            .map(|(_, deadline, _)| *deadline)
            .min()
    }
    pub(crate) fn accept(&mut self, value: Value) -> Result<Message<A>, Error> {
        if self.deadline().is_some_and(|d| Instant::now() >= d) {
            return Err(Error::Timeout);
        }
        if let Some(id) = value.get("id") {
            let id = id.as_u64().ok_or(Error::Protocol)?;
            let (session, _, action) = self.commands.remove(&id).ok_or(Error::Protocol)?;
            if value
                .get("sessionId")
                .map(Value::as_str)
                .unwrap_or(Some(""))
                != Some(session.as_str())
                || value.get("error").is_some()
                || !value.get("result").is_some_and(Value::is_object)
            {
                return Err(Error::Protocol);
            }
            Ok(Message::Response {
                action,
                result: value["result"].clone(),
            })
        } else if value["method"].is_string() && value["params"].is_object() {
            Ok(Message::Event(value))
        } else {
            Err(Error::Protocol)
        }
    }
}

pub(crate) struct Client<A> {
    pipe: Transport,
    pending: Pending<A>,
    next_id: u64,
}
impl<A> Client<A> {
    pub(crate) fn new(pipe: Transport) -> Self {
        Self {
            pipe,
            pending: Pending::default(),
            next_id: 0,
        }
    }
    pub(crate) fn send(
        &mut self,
        session: &str,
        method: &str,
        params: Value,
        action: A,
    ) -> Result<(), Error> {
        self.next_id = self.next_id.checked_add(1).ok_or(Error::Protocol)?;
        self.pending.insert(self.next_id, session, action)?;
        let mut message = json!({"id":self.next_id,"method":method,"params":params});
        if !session.is_empty() {
            message["sessionId"] = json!(session);
        }
        self.pipe
            .send(&message, self.pending.deadline().ok_or(Error::Protocol)?)
    }
    pub(crate) fn next(&mut self) -> Result<Message<A>, Error> {
        // With no outstanding command, an idle kiosk has no protocol deadline.
        // Transport still checks signals and bounds any partially received frame.
        loop {
            let deadline = self.pending.deadline();
            match self
                .pipe
                .receive(deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600)))
            {
                Err(Error::Timeout) if deadline.is_none() && !self.pipe.has_partial_frame() => {
                    continue;
                }
                result => return self.pending.accept(result?),
            }
        }
    }
}
