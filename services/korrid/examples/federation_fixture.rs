//! Emulator-only fixture: real host router and disk identity, executable systemd seam.
//! The fixed person signer is clients/android/signer-test/Bip340EventSigner (secret 3).
//! Device keys and all mutable state are fresh for each run. No live service is used.
use korrid::identity::{DeviceIdentity, OwnerStatementStatus};
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
    types::Timestamp,
};
use std::{fs, path::PathBuf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args_os().nth(1).ok_or("fixture root required")?);
    if !root.is_absolute() || root.exists() {
        return Err("fixture root must be new and absolute".into());
    }
    fs::create_dir(&root)?;
    let private = root.join("private");
    let mut device = DeviceIdentity::load_or_create(&private)?;
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
    // Existing HostConfig TOML schema; the command is recorded, never executed.
    let config = root.join("host.toml");
    fs::write(&config, "label = \"federation-acceptance\"\n[[games]]\nid = \"acceptance-game\"\ntitle = \"Acceptance Game\"\ncommand = [\"/bin/true\"]\n")?;
    let (router, _) =
        korrid::host_routers_with_storage_and_private(&config, None::<PathBuf>, &private);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    fs::write(
        root.join("ready.tmp"),
        format!(
            "{port} {}\n",
            device.device_public_key().ok_or("device key")?
        ),
    )?;
    fs::rename(root.join("ready.tmp"), root.join("ready"))?;
    axum::serve(listener, router).await?;
    Ok(())
}
