//! Exercise the actual localhost RPC and Rust-owned wire, without a compositor.
//! This is its own test process: display facts cannot affect other test targets.
#[tokio::test]
async fn surface_display_opt_in_round_trips_over_localhost_http() {
    let facts = serde_json::json!([
        {"name":"DSI-2","main":true,"x":0,"y":0,"width":640,"height":480,"touch":"/i2c@fe5c0000/touchscreen@14"},
        {"name":"DSI-1","main":false,"x":0,"y":480,"width":640,"height":480,"touch":"/i2c@fe5e0000/touchscreen@14"}
    ]);
    std::env::set_var("KORRID_DISPLAYS", facts.to_string());
    let root = tempfile::tempdir().unwrap();
    let private_parent = tempfile::tempdir().unwrap();
    let private = private_parent.path().join("private");
    let app = korrid::router_with_capability_and_roots(
        "display-test-capability",
        "https://portal.example",
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
    for (scope, count) in [("main", 1), ("all", 2)] {
        let response = client.post(format!("http://{address}/rpc"))
            .bearer_auth("display-test-capability")
            .header("origin", "https://portal.example")
            .json(&serde_json::json!({"_tag":"app.surface.displays","payload":{"displays":scope,"overlay":false}}))
            .send().await.unwrap();
        assert!(response.status().is_success());
        let reply: serde_json::Value = response.json().await.unwrap();
        assert_eq!(reply["outcome"]["_tag"], "Ok");
        let displays = reply["outcome"]["payload"].as_array().unwrap();
        assert_eq!(displays.len(), count);
        assert_eq!(
            displays[0],
            serde_json::json!({"id":"DSI-2","main":true,"x":0,"y":0,"width":640,"height":480,"touch":true})
        );
        if count == 2 {
            assert_eq!(displays[1]["y"], 480);
            assert_eq!(displays[1]["id"], "DSI-1");
        }
    }
    server.abort();
}
