//! Emulator-only fixture: real host, one federation authority, joined discovery.
//! The fixed person signer uses secret 3.
//! Only the test process can restart this fixture. No live service is used.
//!
//! Two uses:
//!
//! - `ROOT PORT RELAYS`: the holder. Sunshine must run on this host's
//!   loopback at its standard ports. Peer traffic always uses iroh. The
//!   holder's game units live in memory (korrid's feature `fixture`): a
//!   launch runs nothing and needs no systemd.
//! - `member DIRECTORY`: a new private state root for another device, bound to
//!   the same owner by the same producer as the holder's own. The Android
//!   app's proof seeds its korrid with it. Prints the device's public key.
//! - `resize HOST_UUID CERTIFICATE WIDTH HEIGHT`: the holder's patch 0046
//!   request for the stream of the client certificate in the file
//!   `CERTIFICATE`, on the certificate-control socket that korrid's
//!   environment names (#476). The Linux viewer's proof asks it for a viewer
//!   that paired by PIN. Prints Sunshine's answer: Applied, Refused or
//!   Unavailable.
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
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::Arc,
};

type Failure = Box<dyn std::error::Error>;

const HOST_CONFIG: &str = "label = \"federation-acceptance\"\n[[games]]\nid = \"acceptance-game\"\ntitle = \"Acceptance Game\"\ncommand = [\"/bin/true\"]\n";
#[derive(Debug, PartialEq)]
enum Command {
    Holder {
        root: PathBuf,
        port: u16,
        relays: RelayList,
    },
    Member {
        directory: PathBuf,
    },
    Resize {
        host_uuid: String,
        certificate: PathBuf,
        width: u32,
        height: u32,
    },
}

fn parse(args: &[String]) -> Result<Command, Failure> {
    let absolute = |value: &str| {
        let path = PathBuf::from(value);
        if !path.is_absolute() || path.is_symlink() {
            return Err::<_, Failure>("fixture paths must be absolute and not linked".into());
        }
        Ok(path)
    };
    match args {
        [command, directory] if command == "member" => {
            let directory = absolute(directory)?;
            if directory.exists() {
                return Err("a member's state directory must be new".into());
            }
            Ok(Command::Member { directory })
        }
        [command, host_uuid, certificate, width, height] if command == "resize" => {
            Ok(Command::Resize {
                host_uuid: host_uuid.clone(),
                certificate: absolute(certificate)?,
                width: width.parse()?,
                height: height.parse()?,
            })
        }
        [root, port, relays] => {
            let relays = RelayList::configured(serde_json::from_str(relays)?)?;
            Ok(Command::Holder {
                root: absolute(root)?,
                port: port.parse()?,
                relays,
            })
        }
        _ => Err("fixture requires root, port and relay JSON, member and a directory, or resize, a host UUID, a certificate file and a size".into()),
    }
}

/// Bind the device whose private state is `private` to the fixture's owner,
/// with the owner statement that korrid's own template asks for. Returns the
/// device's public key.
fn bind_to_fixture_owner(private: &Path) -> Result<String, Failure> {
    fs::DirBuilder::new().mode(0o700).create(private)?;
    let mut device = DeviceIdentity::load_or_create(private)?;
    let owner = Keys::parse(&format!("{:064x}", 3))?;
    let template =
        device.owner_statement_template(OwnerStatementStatus::Owned, Timestamp::now().as_secs())?;
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
    device.apply_signed_owner_binding(&template, &owner.public_key().to_hex(), &event.as_json())?;
    Ok(device.device_public_key().ok_or("device key")?.to_owned())
}

/// The holder publishes its authenticated transport key, never an address.
fn holder_inputs(relays: RelayList) -> DiscoveryInputs {
    DiscoveryInputs {
        relays: Some(relays),
    }
}

#[tokio::main]
async fn main() -> Result<(), Failure> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match parse(&args)? {
        Command::Member { directory } => {
            println!("{}", bind_to_fixture_owner(&directory)?);
            Ok(())
        }
        Command::Resize {
            host_uuid,
            certificate,
            width,
            height,
        } => {
            let certificate = fs::read_to_string(certificate)?;
            let display = korrid::launcher::plugin_launch::PluginDisplay { width, height };
            match korrid::fixture_resize_stream(&host_uuid, &certificate, display) {
                Ok(answer) => {
                    println!("{answer:?}");
                    Ok(())
                }
                Err(failure) => Err(format!("{}: {}", failure.code, failure.message).into()),
            }
        }
        Command::Holder { root, port, relays } => holder(root, port, relays).await,
    }
}

async fn holder(root: PathBuf, port: u16, relays: RelayList) -> Result<(), Failure> {
    let private = root.join("private");
    let config = root.join("host.toml");
    let storage = root.join("storage");
    if port == 0 {
        fs::create_dir(&root)?;
        fs::DirBuilder::new().mode(0o700).create(&storage)?;
        bind_to_fixture_owner(&private)?;
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
    let inputs = holder_inputs(relays);
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
    // In-memory game units (feature `fixture`): the holder launches its game
    // as a host does, and nothing runs.
    let (router, _, _, _) =
        korrid::fixture_host_routers_with_federation(&config, storage, &private, resources);
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

#[cfg(test)]
mod tests {
    use super::*;
    use korrid::identity::IdentityState;

    /// The public key of secret 3, the fixture's person signer.
    const OWNER: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn relays() -> String {
        "[\"ws://127.0.0.1:7000\"]".into()
    }

    #[test]
    fn a_member_is_a_new_device_owned_by_the_fixtures_person() {
        let scratch = tempfile::tempdir().unwrap();
        let first = scratch.path().join("first");
        let device = bind_to_fixture_owner(&first).unwrap();
        // korrid opens the seeded state as its own and reads the binding.
        let resources = FederationResources::open(&first).unwrap();
        match resources.credentials.identity_snapshot().unwrap().state() {
            IdentityState::Owned {
                device_public_key,
                owner_public_key,
                ..
            } => {
                assert_eq!(device_public_key, &device);
                assert_eq!(owner_public_key, OWNER);
            }
            other => panic!("not owned: {other:?}"),
        }
        let second = bind_to_fixture_owner(&scratch.path().join("second")).unwrap();
        assert_ne!(device, second, "each member is its own device");
    }

    #[test]
    fn member_takes_one_new_absolute_directory() {
        let scratch = tempfile::tempdir().unwrap();
        let fresh = scratch.path().join("member");
        assert_eq!(
            parse(&args(&["member", fresh.to_str().unwrap()])).unwrap(),
            Command::Member { directory: fresh }
        );
        for refused in [
            args(&["member"]),
            args(&["member", "relative/state"]),
            // An existing directory is another device's state.
            args(&["member", scratch.path().to_str().unwrap()]),
            args(&["member", "/tmp/a", "/tmp/b"]),
        ] {
            assert!(parse(&refused).is_err(), "{refused:?}");
        }
    }

    #[test]
    fn resize_takes_a_host_an_absolute_certificate_file_and_a_size() {
        assert_eq!(
            parse(&args(&[
                "resize",
                "host-uuid",
                "/tmp/client.pem",
                "720",
                "1280"
            ]))
            .unwrap(),
            Command::Resize {
                host_uuid: "host-uuid".into(),
                certificate: PathBuf::from("/tmp/client.pem"),
                width: 720,
                height: 1280,
            }
        );
        for refused in [
            args(&["resize", "host-uuid", "client.pem", "720", "1280"]),
            args(&["resize", "host-uuid", "/tmp/client.pem", "720", "tall"]),
            args(&["resize", "host-uuid", "/tmp/client.pem", "720"]),
        ] {
            assert!(parse(&refused).is_err(), "{refused:?}");
        }
    }

    #[test]
    fn the_holder_accepts_only_its_root_port_and_record_relays() {
        let Command::Holder { root, port, .. } =
            parse(&args(&["/tmp/holder", "0", &relays()])).unwrap()
        else {
            panic!("not a holder");
        };
        assert_eq!((root, port), (PathBuf::from("/tmp/holder"), 0));
        for refused in [
            args(&["relative", "0", &relays()]),
            args(&["/tmp/holder", "port", &relays()]),
            args(&["/tmp/holder", "0", "[\"ws://192.0.2.1:7000\"]"]),
            args(&["/tmp/holder", "0", &relays(), "192.0.2.7:47989"]),
            args(&["/tmp/holder", "0"]),
        ] {
            assert!(parse(&refused).is_err(), "{refused:?}");
        }
    }
    #[test]
    fn the_holder_supplies_record_relays_not_network_addresses() {
        let relays = RelayList::configured(vec!["ws://127.0.0.1:7000".into()]).unwrap();
        assert_eq!(
            holder_inputs(relays.clone()),
            DiscoveryInputs {
                relays: Some(relays)
            }
        );
    }
}
