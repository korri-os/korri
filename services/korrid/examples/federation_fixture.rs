//! Emulator-only fixture: real host, one federation authority, joined discovery.
//! The fixed person signer uses secret 3.
//! Only the test process can restart this fixture. No live service is used.
//!
//! Two uses:
//!
//! - `ROOT PORT RELAYS [MOONLIGHT]`: the holder. Its one game streams from the
//!   Sunshine at `MOONLIGHT`, the address of a private sender (stream-cadence's
//!   sender fixture, #434 D9). Without it the address is `127.0.0.1:9`, which
//!   serves no video: the session-preparation flow needs none.
//! - `member DIRECTORY`: a new private state root for another device, bound to
//!   the same owner by the same producer as the holder's own. The Android
//!   app's proof seeds its korrid with it. Prints the device's public key.
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
/// The holder's Moonlight address when none is given: the discard port.
const NO_VIDEO: &str = "127.0.0.1:9";

#[derive(Debug, PartialEq)]
enum Command {
    Holder {
        root: PathBuf,
        port: u16,
        relays: RelayList,
        moonlight_address: String,
    },
    Member {
        directory: PathBuf,
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
        [root, port, relays, moonlight @ ..] if moonlight.len() <= 1 => {
            let relays = RelayList::configured(serde_json::from_str(relays)?)?;
            Ok(Command::Holder {
                root: absolute(root)?,
                port: port.parse()?,
                relays,
                moonlight_address: moonlight.first().map_or(NO_VIDEO, String::as_str).to_owned(),
            })
        }
        _ => Err("fixture requires root, port, relay JSON and an optional Moonlight address, or member and a directory".into()),
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

/// What the holder publishes: its loopback endpoint, its label and the
/// address of the Sunshine that its game streams from.
fn holder_inputs(relays: RelayList, port: u16, moonlight_address: &str) -> DiscoveryInputs {
    DiscoveryInputs {
        relays: Some(relays),
        advertised_endpoints: vec![format!("http://127.0.0.1:{port}")],
        // Existing HostConfig label and UpstreamHostConfig Moonlight address semantics.
        label: Some("federation-acceptance".into()),
        moonlight_address: Some(moonlight_address.into()),
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
        Command::Holder {
            root,
            port,
            relays,
            moonlight_address,
        } => holder(root, port, relays, moonlight_address).await,
    }
}

async fn holder(
    root: PathBuf,
    port: u16,
    relays: RelayList,
    moonlight_address: String,
) -> Result<(), Failure> {
    let private = root.join("private");
    let config = root.join("host.toml");
    if port == 0 {
        fs::create_dir(&root)?;
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
    let inputs = holder_inputs(relays, port, &moonlight_address);
    // The publication's own bounds, before anything is published or ready.
    inputs
        .validate()
        .map_err(|_| "the Moonlight address cannot be published")?;
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
    let (router, _, _, _) =
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
    fn the_holder_streams_from_the_given_sunshine_or_from_none() {
        let given = parse(&args(&["/tmp/holder", "0", &relays(), "192.0.2.7:47989"])).unwrap();
        let Command::Holder {
            root,
            port,
            moonlight_address,
            ..
        } = given
        else {
            panic!("not a holder");
        };
        assert_eq!((root, port), (PathBuf::from("/tmp/holder"), 0));
        assert_eq!(moonlight_address, "192.0.2.7:47989");
        let Command::Holder {
            moonlight_address, ..
        } = parse(&args(&["/tmp/holder", "41000", &relays()])).unwrap()
        else {
            panic!("not a holder");
        };
        assert_eq!(moonlight_address, NO_VIDEO);
        for refused in [
            args(&["relative", "0", &relays()]),
            args(&["/tmp/holder", "port", &relays()]),
            args(&["/tmp/holder", "0", "[\"ws://192.0.2.1:7000\"]"]),
            args(&["/tmp/holder", "0", &relays(), "a", "b"]),
            args(&["/tmp/holder", "0"]),
        ] {
            assert!(parse(&refused).is_err(), "{refused:?}");
        }
    }

    #[test]
    fn the_holder_publishes_its_loopback_endpoint_and_its_sunshine() {
        let relays = RelayList::configured(vec!["ws://127.0.0.1:7000".into()]).unwrap();
        let inputs = holder_inputs(relays.clone(), 41000, "192.0.2.7:47989");
        assert_eq!(
            inputs,
            DiscoveryInputs {
                relays: Some(relays.clone()),
                advertised_endpoints: vec!["http://127.0.0.1:41000".into()],
                label: Some("federation-acceptance".into()),
                moonlight_address: Some("192.0.2.7:47989".into()),
            }
        );
        inputs.validate().unwrap();
        // The holder refuses to start with an address it cannot publish.
        for refused in ["", " ", "192.0.2.7:47989\n"] {
            assert!(holder_inputs(relays.clone(), 41000, refused)
                .validate()
                .is_err());
        }
    }
}
