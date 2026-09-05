//! Emulator-only fixture: real host, one federation authority, joined discovery.
//! The fixed person signer is clients/android/signer-test/Bip340EventSigner (secret 3).
//! Only the test process can restart this fixture. No live service is used.
use korrid::{
    federation::coordinator::{Discovery, DiscoveryInputs, DiscoveryTiming, FederationResources},
    identity::{DeviceIdentity, OwnerStatementStatus},
    relay::{RelayList, WebSocketRelayTransport},
};
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
    types::Timestamp,
};
use std::{fs, path::PathBuf, sync::Arc};

const HOST_CONFIG: &str = "label = \"federation-acceptance\"\n[[games]]\nid = \"acceptance-game\"\ntitle = \"Acceptance Game\"\ncommand = [\"/bin/true\"]\n";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("fixture requires root, port, and relay JSON".into());
    }
    let root = PathBuf::from(&args[0]);
    let port: u16 = args[1].parse()?;
    let relays = RelayList::configured(serde_json::from_str(&args[2])?)?;
    if !root.is_absolute() || root.is_symlink() {
        return Err("fixture root must be absolute and not linked".into());
    }
    let private = root.join("private");
    let config = root.join("host.toml");
    if port == 0 {
        fs::create_dir(&root)?;
        let mut device = DeviceIdentity::load_or_create(&private)?;
        let owner = Keys::parse(&format!("{:064x}", 3))?;
        let template = device
            .owner_statement_template(OwnerStatementStatus::Owned, Timestamp::now().as_secs())?;
        let value: serde_json::Value = serde_json::from_str(&template)?;
        let tags: Vec<Vec<String>> = serde_json::from_value(value["tags"].clone())?;
        let event = EventBuilder::new(Kind::Custom(30_078), "")
            .tags(
                tags.into_iter()
                    .map(Tag::parse)
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .custom_created_at(Timestamp::from(
                value["created_at"].as_u64().ok_or("timestamp")?,
            ))
            .finalize(&owner)?;
        device.apply_signed_owner_binding(
            &template,
            &owner.public_key().to_hex(),
            &event.as_json(),
        )?;
        // Existing HostConfig TOML schema; helper records but never runs the command.
        fs::write(&config, HOST_CONFIG)?;
    } else if fs::read_to_string(&config)? != HOST_CONFIG || !private.is_dir() {
        return Err("restart requires the original fixture state".into());
    }
    let resources = FederationResources::open(&private)?;
    let device_key = resources
        .credentials
        .identity_snapshot()?
        .device_public_key()
        .ok_or("device key")?
        .to_owned();
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let port = listener.local_addr()?.port();
    let inputs = DiscoveryInputs {
        relays: Some(relays),
        advertised_endpoints: vec![format!("http://127.0.0.1:{port}")],
        // Existing HostConfig label and UpstreamHostConfig Moonlight address semantics.
        label: Some("federation-acceptance".into()),
        moonlight_address: Some("127.0.0.1:9".into()),
    };
    let discovery = Discovery::new(
        resources.directory.clone(),
        resources.credentials.clone(),
        Arc::new(move || Ok(inputs.clone())),
        Arc::new(WebSocketRelayTransport::new()),
        DiscoveryTiming::default(),
        Arc::new(|| Timestamp::now().as_secs()),
    )
    .spawn();
    let control = discovery.control();
    let (router, _) =
        korrid::host_routers_with_federation(&config, None::<PathBuf>, &private, resources, None);
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    fs::write(root.join("ready.tmp"), format!("{port} {device_key}\n"))?;
    fs::rename(root.join("ready.tmp"), root.join("ready"))?;
    let result = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = terminate.recv() => {},
                _ = tokio::signal::ctrl_c() => {},
            }
            control.cancel();
        })
        .await;
    discovery.shutdown().await?;
    result?;
    println!("fixture HTTP and discovery joined");
    Ok(())
}
