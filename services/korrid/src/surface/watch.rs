//! `app.surface.watch` (native surfaces contract, section 6): the shell
//! selects it with the second frame on the authenticated native-input
//! WebSocket. korrid pushes `{"kind":"model","revision":"<n>","model":…}`
//! whenever the surface's model changes and sends no further model until the
//! shell answers `{"kind":"ack","revision":"<n>"}`. Each push is the whole
//! model. Host-only `localViewer` facts share this authenticated connection,
//! are resent on attach, and are not blocked by a model awaiting its ACK.

use std::{sync::Arc, time::Duration};

use axum::extract::ws::{Message, WebSocket};
use korri_surface_treaty::SurfaceModel;
use serde::{Deserialize, Serialize};
use tokio::{sync::watch, time::timeout};

use super::SurfaceHub;

/// A whole model can be large; a stalled shell still releases the socket.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// The shell must acknowledge a push within this time.
const ACK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Serialize)]
#[serde(tag = "kind", rename = "model")]
struct Push<'a> {
    revision: String,
    model: &'a SurfaceModel,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ack {
    kind: AckKind,
    revision: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum AckKind {
    Ack,
}

enum Incoming {
    Ack(String),
    Ignore,
    Close,
}

async fn receive(socket: &mut WebSocket) -> Incoming {
    match socket.recv().await {
        Some(Ok(Message::Text(text))) => match serde_json::from_str::<Ack>(&text) {
            Ok(Ack {
                kind: AckKind::Ack,
                revision,
            }) => Incoming::Ack(revision),
            Err(_) => Incoming::Close,
        },
        Some(Ok(Message::Ping(_) | Message::Pong(_))) => Incoming::Ignore,
        _ => Incoming::Close,
    }
}

/// Host-only message: it must not enter SurfaceModel or surface package logic.
#[derive(Serialize)]
#[serde(tag = "kind", rename = "localViewer", rename_all = "camelCase")]
struct LocalViewer {
    presenting: bool,
}

async fn send_viewer(socket: &mut WebSocket, presenting: &mut watch::Receiver<bool>) -> bool {
    let text = serde_json::to_string(&LocalViewer {
        presenting: *presenting.borrow_and_update(),
    })
    .expect("a boolean host fact serializes");
    matches!(
        timeout(WRITE_TIMEOUT, socket.send(Message::Text(text.into()))).await,
        Ok(Ok(()))
    )
}

/// Serve one watch until the shell closes, misbehaves or stalls.
pub async fn serve(
    mut socket: WebSocket,
    hub: Arc<SurfaceHub>,
    surface_id: String,
    mut presenting: watch::Receiver<bool>,
) {
    if !send_viewer(&mut socket, &mut presenting).await {
        return;
    }
    let mut changes = hub.subscribe();
    let mut last: Option<serde_json::Value> = None;
    loop {
        changes.borrow_and_update();
        let revision = hub.revision();
        let Ok(model) = hub.model(&surface_id) else {
            return;
        };
        let Ok(value) = serde_json::to_value(&model) else {
            return;
        };
        if last.as_ref() != Some(&value) {
            let text = match serde_json::to_string(&Push {
                revision: revision.to_string(),
                model: &model,
            }) {
                Ok(text) => text,
                Err(_) => return,
            };
            let sent = timeout(WRITE_TIMEOUT, socket.send(Message::Text(text.into()))).await;
            if !matches!(sent, Ok(Ok(()))) {
                return;
            }
            last = Some(value);
            // No new push until this exact revision is acknowledged.
            let acknowledged = timeout(ACK_TIMEOUT, async {
                loop {
                    tokio::select! {
                        changed = presenting.changed() => {
                            if changed.is_err() || !send_viewer(&mut socket, &mut presenting).await {
                                return false;
                            }
                        }
                        incoming = receive(&mut socket) => match incoming {
                            Incoming::Ack(acked) if acked == revision.to_string() => return true,
                            Incoming::Ack(_) | Incoming::Close => return false,
                            Incoming::Ignore => {}
                        }
                    }
                }
            })
            .await;
            if !matches!(acknowledged, Ok(true)) {
                return;
            }
            // A change that arrived while waiting is read on the next turn.
            if changes.has_changed().unwrap_or(false) {
                continue;
            }
        }
        tokio::select! {
            changed = presenting.changed() => {
                if changed.is_err() || !send_viewer(&mut socket, &mut presenting).await {
                    return;
                }
            }
            changed = changes.changed() => if changed.is_err() { return },
            incoming = receive(&mut socket) => match incoming {
                Incoming::Ignore => {}
                // Nothing is outstanding: an ack now is a protocol error.
                Incoming::Ack(_) | Incoming::Close => return,
            },
        }
    }
}
