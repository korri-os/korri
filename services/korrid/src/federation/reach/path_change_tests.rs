//! Root-only proof on aka. Two network namespaces each have two independent
//! links. Removing the selected link must not replace the iroh connection.
use super::*;
use std::{path::Path, process::Command, time::Duration};

fn ip(args: &[&str]) {
    assert!(
        Command::new("ip").args(args).status().unwrap().success(),
        "ip {args:?}"
    );
}
struct Namespaces([String; 2]);
impl Drop for Namespaces {
    fn drop(&mut self) {
        for name in &self.0 {
            let _ = Command::new("ip").args(["netns", "del", name]).status();
        }
    }
}
fn wait_file(path: &Path) -> String {
    let start = std::time::Instant::now();
    loop {
        if let Ok(value) = std::fs::read_to_string(path) {
            return value;
        }
        assert!(
            start.elapsed() < Duration::from_secs(45),
            "waiting for {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "requires root and iproute2 on aka; creates isolated network namespaces"]
fn connection_survives_removing_the_selected_local_path() {
    let suffix = hex::encode(rand::random::<[u8; 4]>());
    let namespaces = Namespaces([format!("korri-a-{suffix}"), format!("korri-b-{suffix}")]);
    for name in &namespaces.0 {
        ip(&["netns", "add", name]);
        ip(&["-n", name, "link", "set", "lo", "up"]);
    }
    for n in 0..2 {
        let a = format!("a{n}{suffix}");
        let b = format!("b{n}{suffix}");
        ip(&["link", "add", &a, "type", "veth", "peer", "name", &b]);
        for (name, link, side) in [(&namespaces.0[0], &a, 1), (&namespaces.0[1], &b, 2)] {
            ip(&["link", "set", link, "netns", name]);
            ip(&["-n", name, "link", "set", link, "name", &format!("path{n}")]);
            ip(&[
                "-n",
                name,
                "addr",
                "add",
                &format!("10.203.{n}.{side}/24"),
                "dev",
                &format!("path{n}"),
            ]);
            ip(&["-n", name, "link", "set", &format!("path{n}"), "up"]);
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    let launch = |name: &str, side: &str| {
        Command::new("ip")
            .args(["netns", "exec", name])
            .arg(&executable)
            .args([
                "--exact",
                "federation::reach::path_change_tests::path_child",
                "--ignored",
                "--nocapture",
            ])
            .env("KORRI_PATH_SIDE", side)
            .env("KORRI_PATH_DIR", dir.path())
            .spawn()
            .unwrap()
    };
    let mut server = launch(&namespaces.0[1], "server");
    let mut client = launch(&namespaces.0[0], "client");
    let selected = wait_file(&dir.path().join("selected"));
    let path = if selected.contains("10.203.0.") {
        "path0"
    } else {
        assert!(selected.contains("10.203.1."), "{selected}");
        "path1"
    };
    for name in &namespaces.0 {
        ip(&["-n", name, "link", "set", path, "down"]);
    }
    std::fs::write(dir.path().join("dropped"), "").unwrap();
    assert!(client.wait().unwrap().success());
    assert!(server.wait().unwrap().success());
}

#[test]
#[ignore = "subprocess of the two-path proof"]
fn path_child() {
    let Ok(side) = std::env::var("KORRI_PATH_SIDE") else {
        return;
    };
    let dir = std::path::PathBuf::from(std::env::var_os("KORRI_PATH_DIR").unwrap());
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
                .alpns(vec![ALPN.to_vec()])
                .relay_mode(iroh::RelayMode::Disabled)
                .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
                .bind()
                .await
                .unwrap();
            if side == "server" {
                let address = endpoint.addr();
                let mut value = format!("{}\n", address.id);
                for address in address.addrs {
                    if let iroh::TransportAddr::Ip(address) = address {
                        value.push_str(&format!("{address}\n"));
                    }
                }
                std::fs::write(dir.join("address"), value).unwrap();
                let connection = tokio::time::timeout(Duration::from_secs(30), async {
                    endpoint.accept().await.unwrap().await.unwrap()
                })
                .await
                .unwrap();
                let router = axum::Router::new().route(
                    "/peer-rpc",
                    axum::routing::post(|| async { "path survived" }),
                );
                let serve = tokio::spawn(tunnel::serve(
                    connection,
                    router,
                    tokio::sync::broadcast::channel(64).0,
                    None,
                ));
                tokio::time::timeout(Duration::from_secs(60), async {
                    while !dir.join("done").exists() {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                })
                .await
                .unwrap();
                serve.abort();
            } else {
                let address = wait_file(&dir.join("address"));
                let mut lines = address.lines();
                let id = lines.next().unwrap().parse::<iroh::EndpointId>().unwrap();
                let mut address = iroh::EndpointAddr::new(id);
                for line in lines {
                    address
                        .addrs
                        .insert(iroh::TransportAddr::Ip(line.parse().unwrap()));
                }
                endpoint
                    .address_lookup()
                    .unwrap()
                    .add(iroh::address_lookup::memory::MemoryLookup::from_endpoint_info([address]));
                let connection = endpoint.connect(id, ALPN).await.unwrap();
                let stable = connection.stable_id();
                async fn call(connection: &iroh::endpoint::Connection) {
                    let mut response = http::post(connection, "/peer-rpc", "{}".into(), None)
                        .await
                        .unwrap();
                    let mut bytes = Vec::new();
                    while let Some(chunk) = response.chunk().await.unwrap() {
                        bytes.extend_from_slice(&chunk);
                    }
                    assert_eq!(bytes, b"path survived");
                }
                call(&connection).await;
                let selected = connection
                    .paths()
                    .iter()
                    .find(|path| path.is_selected())
                    .unwrap()
                    .remote_addr()
                    .to_string();
                std::fs::write(dir.join("selected"), selected).unwrap();
                tokio::time::timeout(Duration::from_secs(30), async {
                    while !dir.join("dropped").exists() {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                })
                .await
                .unwrap();
                tokio::time::timeout(Duration::from_secs(30), call(&connection))
                    .await
                    .unwrap();
                assert_eq!(stable, connection.stable_id());
                assert!(connection.close_reason().is_none());
                std::fs::write(dir.join("done"), "").unwrap();
            }
            endpoint.close().await;
        });
}
