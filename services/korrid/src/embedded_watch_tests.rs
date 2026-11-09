//! The real shell's transport against public embedded startup. A real signed
//! peer supplies the game; a gate keeps its catalog pending until Loading is seen.
use crate::*;
use futures::{SinkExt, StreamExt};
use std::{os::unix::fs::PermissionsExt, time::Duration};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

struct Stop;
impl Drop for Stop {
    fn drop(&mut self) {
        let _ = stop_local_server();
    }
}
struct Serving(tokio::task::JoinHandle<()>);
impl Drop for Serving {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[test]
fn embedded_watch_authenticates_and_pushes_federated_loading_ready_and_acked_updates() {
    let _guard = crate::tests::embedded_server_guard();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let owner = nostr::key::Keys::generate();
        let owner_secret = owner.secret_key().to_secret_hex();
        let peer_root = tempfile::tempdir().unwrap();
        let peer_identity =
            peer_rpc::test_owned_identity(peer_root.path(), &"11".repeat(32), &owner_secret);
        let peer_key = peer_identity.device_public_key().unwrap().to_owned();
        let config = peer_root.path().join("host.toml");
        std::fs::write(
            &config,
            r#"label = "Other device"
[[games]]
id = "remote-game"
title = "A real peer game"
command = ["never-run"]
"#,
        )
        .unwrap();
        let (release, pending) = tokio::sync::watch::channel(false);
        let router = secure_host_router_with_in_memory_units_at(
            &config,
            peer_root.path(),
            peer_rpc::unix_time(),
        )
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let mut pending = pending.clone();
                async move {
                    loop {
                        let released = *pending.borrow_and_update();
                        if released {
                            break;
                        }
                        pending.changed().await.unwrap();
                    }
                    next.run(request).await
                }
            },
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer_url = format!("http://{}", listener.local_addr().unwrap());
        let _peer = Serving(tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }));
        let readable = tempfile::tempdir().unwrap();
        // Existing UpstreamHostConfig serde shape, with the real peer's key.
        std::fs::write(
            readable.path().join("upstreams.json"),
            serde_json::to_vec(&serde_json::json!([{
                "label": "Other device", "kind": "native", "baseUrl": peer_url,
                "devicePublicKey": peer_key,
                // Secure static records require this metadata (upstreams::build).
                // Only catalog RPC is exercised; no Moonlight connection is made.
                "moonlightAddress": peer_url.strip_prefix("http://").unwrap()
            }]))
            .unwrap(),
        )
        .unwrap();
        let private = tempfile::tempdir().unwrap();
        std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        peer_rpc::test_owned_identity(private.path(), &"22".repeat(32), &owner_secret);
        let port = start_local_shell_server(
            readable.path().to_str().unwrap(),
            private.path().to_str().unwrap(),
            EmbeddedPlatform::default(),
        )
        .unwrap();
        let origin = format!("http://127.0.0.1:{port}");
        let _stop = Stop;
        let capability = zeroize::Zeroizing::new(local_server_capability().unwrap());
        let request = || {
            let mut request = format!("ws://127.0.0.1:{port}/")
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("Origin", origin.parse().unwrap());
            request
        };
        for other in [
            None,
            Some("https://shell.korri.example"),
            Some("http://127.0.0.1:1"),
        ] {
            let mut wrong = request();
            wrong.headers_mut().remove("Origin");
            if let Some(origin) = other {
                wrong
                    .headers_mut()
                    .insert("Origin", origin.parse().unwrap());
            }
            let error = tokio_tungstenite::connect_async(wrong).await.unwrap_err();
            let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
                panic!("unexpected origin rejection: {error}");
            };
            assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
        }
        let watch =
            serde_json::json!({"_tag": "app.surface.watch", "payload": {"surfaceId": "zone"}})
                .to_string();
        for wrong in [None, Some("Bearer wrong-capability")] {
            let (mut socket, _) = tokio_tungstenite::connect_async(request())
                .await
                .expect("embedded startup must expose the existing root WebSocket");
            if let Some(bearer) = wrong {
                socket.send(Message::Text(bearer.into())).await.unwrap();
            }
            let _ = socket.send(Message::Text(watch.clone().into())).await;
            let rejected = tokio::time::timeout(Duration::from_secs(5), socket.next())
                .await
                .unwrap();
            assert!(
                matches!(rejected, None | Some(Ok(Message::Close(_))) | Some(Err(_))),
                "unauthenticated watch received data"
            );
        }
        let (mut socket, _) = tokio_tungstenite::connect_async(request()).await.unwrap();
        socket
            .send(Message::Text(
                format!("Bearer {}", capability.as_str()).into(),
            ))
            .await
            .unwrap();
        socket.send(Message::Text(watch.into())).await.unwrap();
        assert_eq!(
            receive(&mut socket).await,
            serde_json::json!({"kind":"localViewer", "presenting":false})
        );
        let first = receive(&mut socket).await;
        assert_eq!(first["kind"], "model");
        assert_eq!(
            first["model"]["catalog"]["_tag"], "Loading",
            "peer must remain gated until the first model"
        );
        release.send(true).unwrap();
        ack(&mut socket, &first).await;
        let ready = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let push = receive(&mut socket).await;
                if push["model"]["catalog"]["_tag"] == "Ready"
                    && push["model"]["onboarding"]["controllers"].is_array()
                {
                    break push;
                }
                ack(&mut socket, &push).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(ready["model"]["catalog"]["total"], 1);
        assert!(
            ready["model"]["onboarding"]["controllers"]
                .as_array()
                .unwrap()
                .is_empty(),
            "an empty input transport must not invent a heard controller"
        );
        assert_eq!(
            ready["model"]["catalog"]["window"][0]["title"],
            "A real peer game"
        );
        let response: RpcResponse = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .post(format!("http://127.0.0.1:{port}/rpc"))
            .bearer_auth(capability.as_str())
            .json(&RpcRequest::SurfaceRequest(
                surface::SurfaceRequestRequest {
                    surface_id: "zone".into(),
                    request: korri_surface_treaty::SurfaceRequest::MoveCatalog {
                        query: korri_surface_treaty::CatalogQuery {
                            text: "not-a-game".into(),
                            ..Default::default()
                        },
                        from: korri_surface_treaty::CatalogFrom::Start,
                        count: 10,
                    },
                },
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(matches!(
            response,
            RpcResponse::SurfaceRequest(surface::SurfaceAcceptedOutcome::Ok(_))
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), socket.next())
                .await
                .is_err(),
            "a push overtook its outstanding ACK"
        );
        ack(&mut socket, &ready).await;
        let updated = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let push = receive(&mut socket).await;
                if push["model"]["catalog"]["query"]["text"] == "not-a-game" {
                    break push;
                }
                ack(&mut socket, &push).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(updated["model"]["catalog"]["total"], 0);
        assert_ne!(updated["revision"], ready["revision"]);
        ack(&mut socket, &updated).await;
        socket.close(None).await.unwrap();
    });
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
async fn receive(socket: &mut Socket) -> serde_json::Value {
    let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(message.to_text().unwrap()).unwrap()
}
async fn ack(socket: &mut Socket, push: &serde_json::Value) {
    socket
        .send(Message::Text(
            serde_json::json!({"kind": "ack", "revision": push["revision"]})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
}

/// Android/JNI supplies an owner-binding coordinator to this same startup API.
/// Keep that existing platform composition working with the added watch route.
#[test]
fn embedded_owner_binding_platform_keeps_its_identity_rpc_and_surface_watch() {
    let _guard = crate::tests::embedded_server_guard();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut platform = EmbeddedPlatform::default();
        platform.owner_binding = Some(owner_binding::OwnerBindingCoordinator::default());
        let origin = "https://android-shell.example";
        let port = start_local_server(
            origin,
            readable.path().to_str().unwrap(),
            private.path().to_str().unwrap(),
            platform,
        )
        .unwrap();
        let _stop = Stop;
        let capability = zeroize::Zeroizing::new(local_server_capability().unwrap());
        let response: RpcResponse = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .post(format!("http://127.0.0.1:{port}/rpc"))
            .bearer_auth(capability.as_str())
            .json(&RpcRequest::OwnerBindingSnapshot(OwnerBindingRequest {}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let RpcResponse::OwnerBindingSnapshot(OwnerBindingOutcome::Ok(snapshot)) = response else {
            panic!("the native owner-binding coordinator became unavailable");
        };
        assert!(matches!(
            snapshot.identity,
            identity::IdentityState::Unowned { .. }
        ));
        let mut request = format!("ws://127.0.0.1:{port}/")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("Origin", origin.parse().unwrap());
        let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        socket
            .send(Message::Text(
                format!("Bearer {}", capability.as_str()).into(),
            ))
            .await
            .unwrap();
        socket
            .send(Message::Text(
                serde_json::json!({"_tag":"app.surface.watch","payload":{"surfaceId":"pico"}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        assert_eq!(
            receive(&mut socket).await,
            serde_json::json!({"kind":"localViewer", "presenting":false})
        );
        let push = receive(&mut socket).await;
        assert_eq!(push["kind"], "model");
        ack(&mut socket, &push).await;
        // The existing JNI stop operation must also finish with a watch open.
        stop_local_server().unwrap();
        // A frame written before shutdown can still be buffered in TCP. Drain
        // it without ACKing; shutdown must close before the 30 s ACK deadline.
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(Ok(message)) = socket.next().await {
                if message.is_close() {
                    return;
                }
            }
        })
        .await
        .expect("embedded stop must close an active surface watch");
    });
}

#[test]
fn embedded_input_edge_feeds_native_socket_and_controller_facts_and_is_joined() {
    use crate::native_input::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    let _guard = crate::tests::embedded_server_guard();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        struct Stopped(Arc<AtomicBool>);
        impl Drop for Stopped {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let stopped = Arc::new(AtomicBool::new(false));
        let mark = stopped.clone();
        let (send, mut commands) = tokio::sync::mpsc::channel::<()>(1);
        let (ready, initialized) = tokio::sync::oneshot::channel();
        let mut platform = EmbeddedPlatform::default();
        platform.native_input = Some(Box::new(move |source| {
            tokio::spawn(async move {
                let _stopped = Stopped(mark);
                source
                    .publish_device(
                        NativeInputDeviceInfo {
                            device_id: "physical-pad".into(),
                            class: NativeInputDeviceClass::Gamepad,
                            name: "Physical pad".into(),
                            axes: None,
                            capabilities: vec![],
                        },
                        vec![],
                    )
                    .unwrap();
                ready.send(()).unwrap();
                while commands.recv().await.is_some() {
                    source
                        .publish_frame(
                            "physical-pad",
                            vec![NativeInputInput {
                                kind: NativeInputInputKind::Input,
                                device_id: "physical-pad".into(),
                                class: NativeInputDeviceClass::Gamepad,
                                input_type: 1.0,
                                code: 304.0,
                                value: 1.0,
                                timestamp: 1.0,
                            }],
                        )
                        .unwrap();
                }
                std::future::pending::<()>().await;
            })
        }));
        let port = start_local_shell_server(
            readable.path().to_str().unwrap(),
            private.path().to_str().unwrap(),
            platform,
        )
        .unwrap();
        let _stop = Stop;
        tokio::time::timeout(Duration::from_secs(5), initialized)
            .await
            .unwrap()
            .unwrap();
        let capability = zeroize::Zeroizing::new(local_server_capability().unwrap());
        let connect = || async {
            let mut request = format!("ws://127.0.0.1:{port}/")
                .into_client_request()
                .unwrap();
            request.headers_mut().insert(
                "Origin",
                format!("http://127.0.0.1:{port}").parse().unwrap(),
            );
            let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
            socket
                .send(Message::Text(
                    format!("Bearer {}", capability.as_str()).into(),
                ))
                .await
                .unwrap();
            socket
        };
        let mut native = connect().await;
        native
            .send(Message::Text(
                serde_json::json!({"classes":["gamepad"]})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        assert_eq!(receive(&mut native).await["kind"], "device-added");
        assert_eq!(
            receive(&mut native).await["kind"],
            "initialization-complete"
        );
        let mut surface = connect().await;
        surface
            .send(Message::Text(
                serde_json::json!({"_tag":"app.surface.watch","payload":{"surfaceId":"zone"}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        assert_eq!(
            receive(&mut surface).await,
            serde_json::json!({"kind":"localViewer", "presenting":false})
        );
        let push = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let push = receive(&mut surface).await;
                if push["model"]["onboarding"]["controllers"].is_array() {
                    break push;
                }
                ack(&mut surface, &push).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            push["model"]["onboarding"]["controllers"],
            serde_json::json!([{"id":"physical-pad","name":"Physical pad","heard":false}])
        );
        ack(&mut surface, &push).await;
        send.send(()).await.unwrap();
        let value = receive(&mut native).await;
        assert_eq!(value["kind"], "input");
        assert_eq!(value["code"].as_f64(), Some(304.0));
        let report = receive(&mut native).await;
        assert_eq!(report["type"].as_f64(), Some(0.0));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let push = receive(&mut surface).await;
                let heard = push["model"]["onboarding"]["controllers"][0]["heard"] == true;
                ack(&mut surface, &push).await;
                if heard {
                    break;
                }
            }
        })
        .await
        .unwrap();
        // Both subscriptions and the edge task are live at shutdown.
        stop_local_server().unwrap();
        assert!(
            stopped.load(Ordering::SeqCst),
            "edge task must be joined before stop returns"
        );
    });
}

#[test]
fn local_viewer_host_fact_crosses_watch_without_model_ack_and_is_resent_on_reconnect() {
    let _guard = crate::tests::embedded_server_guard();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let (send, source) = tokio::sync::oneshot::channel();
        let platform = EmbeddedPlatform {
            native_input: Some(Box::new(move |source| {
                send.send(source).ok().unwrap();
                tokio::spawn(std::future::pending())
            })),
            ..Default::default()
        };
        let port = start_local_shell_server(
            readable.path().to_str().unwrap(),
            private.path().to_str().unwrap(),
            platform,
        )
        .unwrap();
        let _stop = Stop;
        let source = source.await.unwrap();
        let capability = zeroize::Zeroizing::new(local_server_capability().unwrap());
        let connect = || async {
            let mut request = format!("ws://127.0.0.1:{port}/")
                .into_client_request()
                .unwrap();
            request.headers_mut().insert(
                "Origin",
                format!("http://127.0.0.1:{port}").parse().unwrap(),
            );
            let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
            socket
                .send(Message::Text(
                    format!("Bearer {}", capability.as_str()).into(),
                ))
                .await
                .unwrap();
            socket
                .send(Message::Text(
                    serde_json::json!({"_tag":"app.surface.watch","payload":{"surfaceId":"zone"}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            socket
        };
        let mut socket = connect().await;
        assert_eq!(
            receive(&mut socket).await,
            serde_json::json!({"kind":"localViewer", "presenting":false})
        );
        let model = receive(&mut socket).await;
        assert_eq!(model["kind"], "model");
        assert!(
            model["model"].get("localViewer").is_none(),
            "surface treaty must not carry this fact"
        );
        // Deliberately leave the model unacknowledged: video readiness/exit must
        // still reach the host while package logic is busy applying that model.
        for presenting in [true, false, true] {
            source.set_local_viewer_presenting(presenting);
            assert_eq!(
                receive(&mut socket).await,
                serde_json::json!({"kind":"localViewer", "presenting":presenting})
            );
        }
        socket.close(None).await.unwrap();
        let mut socket = connect().await;
        assert_eq!(
            receive(&mut socket).await,
            serde_json::json!({"kind":"localViewer", "presenting":true})
        );
        let model = receive(&mut socket).await;
        ack(&mut socket, &model).await;
        source.set_local_viewer_presenting(false);
        loop {
            let push = receive(&mut socket).await;
            if push["kind"] == "localViewer" {
                assert_eq!(push["presenting"], false);
                break;
            }
            ack(&mut socket, &push).await;
        }
    });
}
