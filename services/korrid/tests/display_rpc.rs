//! Exercise the actual localhost RPC and Rust-owned wire, without a compositor.
//! This is its own test process: display facts cannot affect other test targets.
//!
//! ADR 0086: `app.surface.displays` answers which windows a package's panes
//! get. Without a compositor nothing is placed, so a package with panes gets
//! them all in one window, and a package without panes gets no windows.
#[tokio::test]
async fn surface_windows_over_localhost_http() {
    let facts = serde_json::json!([
        {"name":"DSI-2","main":true,"unwatched":false,"mode":"640x480@60Hz","x":0,"y":0,"width":640,"height":480,"touch":"/i2c@fe5c0000/touchscreen@14"},
        {"name":"DSI-1","main":false,"unwatched":false,"mode":"640x480@60Hz","x":0,"y":480,"width":640,"height":480,"touch":"/i2c@fe5e0000/touchscreen@14"}
    ]);
    std::env::set_var("KORRID_DISPLAYS", facts.to_string());
    let root = tempfile::tempdir().unwrap();
    let private_parent = tempfile::tempdir().unwrap();
    let private = private_parent.path().join("private");
    let app = korrid::router_with_capability_and_roots(
        "display-test-capability",
        "https://shell.example",
        root.path(),
        &private,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::new();
    let panes =
        serde_json::json!({"panes": ["stage", "floor"], "arrangement": "upper-above-lower"});
    for (payload, windows) in [
        (
            serde_json::json!({"overlay": false}),
            serde_json::Value::Null,
        ),
        (
            serde_json::json!({"panes": panes, "overlay": false}),
            serde_json::json!([{"panes": ["stage", "floor"]}]),
        ),
        (
            serde_json::json!({"panes": panes, "overlay": false, "check": true}),
            serde_json::json!([{"panes": ["stage", "floor"]}]),
        ),
    ] {
        let response = client
            .post(format!("http://{address}/rpc"))
            .bearer_auth("display-test-capability")
            .header("origin", "https://shell.example")
            .json(&serde_json::json!({"_tag":"app.surface.displays","payload": payload}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{payload}");
        let reply: serde_json::Value = response.json().await.unwrap();
        assert_eq!(reply["outcome"]["_tag"], "Ok", "{payload}: {reply}");
        assert_eq!(reply["outcome"]["payload"], windows, "{payload}: {reply}");
    }
    // The old wire is gone: a display scope is refused.
    let response = client
        .post(format!("http://{address}/rpc"))
        .bearer_auth("display-test-capability")
        .header("origin", "https://shell.example")
        .json(&serde_json::json!({"_tag":"app.surface.displays","payload":{"displays":"all","overlay":false}}))
        .send()
        .await
        .unwrap();
    assert!(!response.status().is_success());
    server.abort();
}
