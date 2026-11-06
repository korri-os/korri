//! Hidden, dev-only workstation operator edge. Sign the existing TV request
//! with the existing NIP-46 implementation; never install a person key on TV.
use crate::{
    identity::{validate_owned_statement_template, DeviceIdentity, OwnerStatementStatus},
    relay::{CoordinatedRelays, WebSocketRelayTransport},
    remote_signer::{
        bunker_relay_list, Nip46PersonSigner, PersonSigner, PersonSignerRequest, PersonSignerState,
    },
};
use std::{
    io::{self, Read},
    path::Path,
    sync::Arc,
};
use zeroize::Zeroizing;

const MAX_BYTES: usize = 64 * 1024;
pub const HELP: &str = "Hidden dev-only workstation operator tool, not a product join flow.\nusage: webos-owner-sign --template FILE --expected-owner PUBLIC_KEY < PRIVATE_BUNKER_URI_FILE > SIGNED_EVENT_FILE\nOnly bunker:// is supported by the existing NIP-46 adapter. Obtain a fresh bunker URL from the owner's signer. It must sign kind 30078. The URI is read from private stdin, never from arguments. The signer's URI supplies its relays. stdout contains only the verified signed public event. No TV operation is performed.";

fn bounded(reader: &mut dyn Read) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read operator input")?;
    if bytes.len() > MAX_BYTES {
        return Err("Operator input exceeds 65536 bytes".into());
    }
    String::from_utf8(bytes).map_err(|_| "Operator input must be UTF-8".into())
}

fn verify_result(template: &str, owner: &str, state: PersonSignerState) -> Result<String, String> {
    let requested =
        validate_owned_statement_template(template).map_err(|error| error.to_string())?;
    let (returned_owner, returned_template, event) = match state {
        PersonSignerState::Approved {
            owner_public_key,
            unsigned_event_template,
            signed_event_json,
        } => (owner_public_key, unsigned_event_template, signed_event_json),
        // Keep signer diagnostics bounded and free of private connection data.
        PersonSignerState::Denied { .. } => return Err("The signer denied the binding".into()),
        _ => return Err("The signer did not return an approved binding".into()),
    };
    if returned_owner != owner || returned_template != template {
        return Err("The signer returned another owner or template".into());
    }
    // This is exactly the public verifier used by identity_cli::import. The
    // NIP-46 adapter already verifies exact template equality before Approved.
    let statement = DeviceIdentity::verify_owner_statement(&event, &requested.device_public_key)
        .map_err(|error| error.to_string())?;
    if statement.status != OwnerStatementStatus::Owned || statement.owner_public_key != owner {
        return Err("The signed event is not an owned statement from the expected owner".into());
    }
    // Independently compare the public signed event with the submitted template
    // before emitting it, using the same event decoder as identity verification.
    let verified = DeviceIdentity::verify_event(&event).map_err(|error| error.to_string())?;
    if verified.created_at != requested.created_at
        || verified.tags != requested.tags
        || verified.content != requested.content
    {
        return Err("The signer returned a different event".into());
    }
    Ok(event)
}

// A signer's denial text is untrusted. It can echo the one-use connection
// secret, so the operator edge must not copy it into stderr or captured logs.
fn connection_failure(_: crate::remote_signer::RemoteSignerError) -> String {
    "Remote signer connection failed. Check its relays and use a fresh bunker URL.".into()
}

pub async fn sign(template: &str, expected_owner: &str, bunker: &str) -> Result<String, String> {
    validate_owned_statement_template(template).map_err(|error| error.to_string())?;
    let owner = nostr::key::PublicKey::from_hex(expected_owner)
        .map_err(|_| "Expected owner must be a valid public key")?
        .to_hex();
    if owner != expected_owner {
        return Err("Expected owner must be a canonical lowercase public key".into());
    }
    // The existing adapter supports signer-initiated bunker:// only, not
    // client-initiated nostrconnect://. Relay selection comes from that token.
    let relays = bunker_relay_list(bunker).map_err(|_| "Invalid bunker connection")?;
    let scratch = tempfile::Builder::new()
        .prefix("korri-webos-owner-sign-")
        .tempdir()
        .map_err(|_| "Could not create private temporary signer state")?;
    let identity =
        DeviceIdentity::load_or_create(scratch.path()).map_err(|error| error.to_string())?;
    let coordinator = Arc::new(
        CoordinatedRelays::new(relays, identity, Arc::new(WebSocketRelayTransport::new()))
            .map_err(|error| error.to_string())?,
    );
    let signer = Nip46PersonSigner::connect_from_bunker(scratch.path(), coordinator, bunker)
        .map_err(|_| "Could not prepare remote signer")?;
    eprintln!("Approve the connection in the owner's signer, then approve the TV owner-binding event (kind 30078). Use the same person key as the existing federation.");
    let operation = async {
        signer
            .establish(crate::peer_rpc::unix_time())
            .await
            .map_err(connection_failure)?;
        let state = signer
            .request(PersonSignerRequest {
                unsigned_event_template: template.into(),
            })
            .await;
        verify_result(template, &owner, state)
    };
    tokio::select! {
        result = operation => result,
        _ = tokio::signal::ctrl_c() => Err("Signing interrupted".into()),
    }
}

pub async fn run(template: &Path, expected_owner: &str) -> Result<String, String> {
    let template =
        bounded(&mut std::fs::File::open(template).map_err(|_| "Could not open template")?)?;
    // Validate public inputs before asking the person or connecting to a relay.
    validate_owned_statement_template(&template).map_err(|error| error.to_string())?;
    let bunker = Zeroizing::new(bounded(&mut io::stdin().lock())?);
    sign(
        &template,
        expected_owner,
        bunker.trim_end_matches(['\r', '\n']),
    )
    .await
}

#[cfg(test)]
#[path = "webos_owner_sign_tests.rs"]
mod tests;
