//! Private input producer over korrid's existing credential-filtered Unix HTTP socket.
//! The only wire types are the receiver coordinator treaty; no browser bearer,
//! public listener, socket path, or independent producer schema is introduced.
use futures_util::{SinkExt, StreamExt};
use korri_input_contract::{SeatReply, SeatRequest, MAX_COORDINATION_BYTES};
use std::{io, path::Path, time::Duration};
use tokio::{net::UnixStream, sync::mpsc, task::JoinHandle};
use tokio_tungstenite::{
    tungstenite::{protocol::WebSocketConfig, Message},
    WebSocketStream,
};

const QUEUE: usize = 256;
const IO_TIMEOUT: Duration = Duration::from_millis(750);
const FEEDBACK_TIMEOUT: Duration = Duration::from_millis(1250);

pub struct PrivateInputChannel {
    requests: mpsc::Sender<SeatRequest>,
    replies: mpsc::Receiver<SeatReply>,
    task: JoinHandle<()>,
}
impl Drop for PrivateInputChannel {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl PrivateInputChannel {
    pub async fn connect(path: &Path) -> io::Result<Self> {
        tokio::time::timeout(IO_TIMEOUT, async {
            let stream = UnixStream::connect(path).await?;
            // Host is only HTTP handshake syntax: the transport is the already
            // configured Unix fd. Do not substitute a localhost TCP connection.
            let config = WebSocketConfig::default()
                .max_message_size(Some(MAX_COORDINATION_BYTES))
                .max_frame_size(Some(MAX_COORDINATION_BYTES));
            let (socket, _) = tokio_tungstenite::client_async_with_config(
                "ws://localhost/",
                stream,
                Some(config),
            )
            .await
            .map_err(|_| io::Error::other("private input upgrade failed"))?;
            let (requests, request_rx) = mpsc::channel(QUEUE);
            let (reply_tx, replies) = mpsc::channel(QUEUE);
            let task = tokio::spawn(serve(socket, request_rx, reply_tx));
            Ok(Self {
                requests,
                replies,
                task,
            })
        })
        .await
        .map_err(|_| io::Error::other("private input upgrade timed out"))?
    }
    pub fn send(&self, request: SeatRequest) -> io::Result<()> {
        if !matches!(
            request,
            SeatRequest::PhysicalConnected { .. }
                | SeatRequest::PhysicalState { .. }
                | SeatRequest::PhysicalDisconnected { .. }
        ) {
            return Err(io::Error::other("nonphysical producer request rejected"));
        }
        self.requests
            .try_send(request)
            .map_err(|_| io::Error::other("private input queue unavailable"))
    }
    pub async fn next(&mut self) -> Option<SeatReply> {
        // A closed actor may still have buffered replies. Those old edges no
        // longer carry live launch authority and must not fire host shortcuts.
        if self.requests.is_closed() {
            return None;
        }
        let reply = self.replies.recv().await;
        if self.requests.is_closed() {
            None
        } else {
            reply
        }
    }
}
async fn serve(
    mut socket: WebSocketStream<UnixStream>,
    mut requests: mpsc::Receiver<SeatRequest>,
    replies: mpsc::Sender<SeatReply>,
) {
    let deadline = tokio::time::sleep(FEEDBACK_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            request = requests.recv() => {
                let Some(request) = request else { break; };
                let Ok(text) = serde_json::to_string(&request) else { break; };
                if text.len() > MAX_COORDINATION_BYTES { break; }
                if !matches!(tokio::time::timeout(IO_TIMEOUT, socket.send(Message::Text(text.into()))).await, Ok(Ok(()))) { break; }
            }
            message = socket.next() => {
                let Some(Ok(Message::Text(text))) = message else { break; };
                let Ok(reply) = serde_json::from_str::<SeatReply>(text.as_ref()) else { break; };
                if reply.failure.is_some() || reply.recovery_required || reply.remote_sources.len() > 16
                    || reply.remote_events.len() > korri_input_contract::MAX_REMOTE_EVENTS { break; }
                deadline.as_mut().reset(tokio::time::Instant::now() + FEEDBACK_TIMEOUT);
                if replies.try_send(reply).is_err() { break; }
            }
        }
    }
    // Drop closes authority. No unbounded close handshake or stale replay.
}
