# Korri identity protocol

Status: accepted for the first identity slice

Source work item: `01M1MT7AFHS742T1GT0R55FEHV`

Protocol source revision: `nostr-protocol/nips@6aeea6093786644e892dd2869fa5b642fddd271d`

## Decision

Korri uses Nostr-format keys for person identity and device identity. Each key uses secp256k1 and BIP-340 Schnorr signatures. Korri does not need a managed account service.

The person controls the person key. Korri does not store that private key. An Android signer supplies signatures through NIP-55. A remote signer supplies signatures through NIP-46.

The Android implementation uses the NIP-55 package-discovery intent, account public-key selection, explicit selected signer package, Activity Result lifecycle, and permission-gated ContentResolver path. Amber is the first named compatible signer, not a package or protocol dependency. Korri persists only the selected signer package, selected public key, and bounded pending public request state. The person key, its generation, and NIP-49 backup remain signer-owned.

Each device stores one device key under the existing private state root. The device key signs daily device traffic. A person key signs only owner statements and later device passes.

## Protocol sources

The first slice uses these rules:

- NIP-01 defines event IDs, event JSON, secp256k1 keys, and BIP-340 signatures.
- NIP-44 version 2 defines encryption between two Nostr keys. The receiver verifies the outer event before decryption.
- NIP-09 defines signed deletion events. Korri uses kind `5` to revoke one exact person-pass event.
- NIP-78 defines kind `30078` for addressable application data. Korri uses this kind for device owner state.
- NIP-46 defines remote signing. It also defines disposable client keys and NIP-44 encrypted requests.
- NIP-55 defines the Android signer interface. The signer keeps the person private key outside Korri.
- NIP-49 defines encrypted export of a person private key. It does not define storage for a device key.
- NIP-59 defines gift wrapping for relay metadata protection. Relay coordination is a later slice.

## Device state

The identity module exposes four states:

- `Unowned`: the device key is valid, but no owner statement exists.
- `Owned`: a valid owner binding names this device key.
- `Revoked`: a newer statement from the same owner revokes this device key.
- `Invalid`: the key file or the owner statement is not safe to use.

An invalid identity does not repair itself. A device reset removes the identity state. The reset flow is outside this slice.

## Owner statement

An owner statement is a NIP-78 addressable event. The event author is the owner public key. The event uses kind `30078` and empty content.

The event has exactly these ordered tags:

```text
["d", "org.korri.device-owner:<device-public-key>"]
["device", "<device-public-key>"]
["status", "owned" | "revoked"]
```

The `d` tag makes the state addressable for one device. The `device` tag makes the target explicit. The `status` tag selects the state.

A new device accepts `owned` as its first owner statement. An owned device accepts a newer statement only from the same owner. A revoked device accepts no owner changes. Normal ownership changes require reset, which creates a new device key. The approved offline test-owner retirement below is a separate administrative exception, not an import or RPC rule.

NIP-01 defines the order for addressable events. A later timestamp wins. At the same timestamp, the event with the lower event ID wins.

## Persistence

The existing private state root is the storage boundary. The identity module owns one fixed subdirectory:

```text
<private-state-root>/identity/
```

The directory has mode `0700`. It contains these files:

```text
device.key
owner.event.json
```

`device.key` contains the 32-byte private key as lowercase hexadecimal text. The file has mode `0600`. The module creates it once and does not replace an invalid key.

`owner.event.json` contains the complete signed NIP-78 event. The file has mode `0600`. The module writes it with a same-directory temporary file, file sync, rename, and directory sync.

The file names follow the fixed-file pattern that the current private state root already uses. The signed NIP event is the producer and the source of truth for owner state.

Android Keystore support and a Linux TPM are outside this slice. The storage adapter can change later without a change to the public identity state.

## Offline test-owner retirement

The user approved the guarded test-to-private-owner cutover and retirement of
**all** old client pairings on 2026-09-06, in decision
`c74a26f8-7924-4af5-838c-f6aa4d2fc8d0` (`approve-cutover-repair`).
The device key, Sunshine host identity, games, and history must remain unchanged.
This costs a maintenance interruption and client re-pairing.

The local administrative entry is:

```text
korrid identity replace-test-owner-offline --expected-device KEY --expected-owner KEY --expected-event ID --new-owner KEY --template PATH --file PATH
```

Supply the arguments in this order. Set the existing `KORRID_PRIVATE_STATE_ROOT`
explicitly to the approved absolute private-state root. Both input paths must
also be absolute. `--template` is the exact unsigned template produced by
`DeviceIdentity::owner_statement_template`; `--file` is its complete signed
owner event. Select `--new-owner` independently through the user's signer,
not by copying the author from an untrusted returned event.

This command only replaces an existing, verified `owned` binding from the known
repository test signer, public key
`f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9`.
The selected new owner must differ. The existing device key, current owner, and
current event ID must equal all three expected-current arguments. Producer
verification checks the signature, event ID, device, selected owner, and exact
unsigned template. Missing or invalid state fails without generating a key.
Normal `import`, owner apply, and peer RPC still deny cross-owner changes.
A stale test-owner import fails after this operation commits.

The command uses the existing `identity/device.key` and
`identity/owner.event.json` format. It changes only the owner event. It does not
create a transfer record, change the device key, or migrate any other state.
It opens every path component without following symlinks. The state root,
identity directory, and each source's direct parent must be `0700`. Files must
be regular, single-link `0600` files with their parent's UID/GID. The identity
directory must have the state root's UID/GID. Inputs are bounded to 65536 bytes;
the key is bounded to the producer's existing 128-byte limit. Ancestors must
belong to root or the private directory owner and must not be group/other
writable, except root-owned sticky directories such as `/tmp`.
Stage immutable public artifacts into approved private files first; do not pass
Nix-store files directly or loosen existing private-state permissions.

Run under root-controlled maintenance, as root or the existing private-state
UID. The replacement retains the current event's UID/GID and `0600` mode.
The device key's bytes, inode, UID/GID, and mode remain unchanged. Same-directory
exclusive temporary creation, file sync, expected-current recheck, atomic rename,
and directory sync follow the existing identity writer pattern. The stored new
event is the exact verified input. A final reopen verifies the new event and
unchanged key. A successful result prints only the existing public identity
state, never the signed event or key bytes.

**Offline exclusive authority is a precondition, not something this command
proves.** Its nonblocking advisory lock on the identity directory excludes only
other offline owner replacement and grant reconciliation invocations. The platform procedure must stop and block
all daemons, importers, JNI processes, launch/pairing paths, and other writers
before invoking it. That procedure must also prove absent federation memory,
retire old Sunshine client trust through Sunshine's producer, preserve replay
and signed revocation evidence, and verify the compatible new-owner generation
before allowing network access. This command does none of those operations.

Any error or interruption keeps authority stopped. Inspect the exact current
binding before recovery. A failed write leaves the complete old or complete new
event; a post-rename sync error is an ambiguous outcome, not permission to retry
or restart the old generation. Interrupted staging can leave an unused private
temporary file. Never restore the old owner event or old client trust after the
cut commits. Software recovery must retain the new owner and revoked old trust.
The preserved key retains its historical public linkage; this operation does
not erase relay history, establish that the key was never copied, or add forward
secrecy.

### Offline stale client-grant reconciliation

After the new binding is installed, the approved cut uses:

```text
korrid identity reconcile-stale-grants-offline --expected-device KEY --expected-owner KEY --expected-event ID
```

Supply arguments in this order. All three values must come from independently
verified **new** ownership evidence. Set `KORRID_PRIVATE_STATE_ROOT` explicitly.
The command reuses the offline no-create reader and directory lock. It requires
an existing private owner, rejects the known test owner, and runs only as the
existing unprivileged private-state UID, never root. Missing identity or
authorization directories fail without creation. File ownership, permissions,
links, and bounds use the same strict offline checks as owner replacement.

`authorization.rs` supplies the existing grant format, signed-evidence parsers,
and reconciliation planner. The command validates the whole grant and signed
revocation inventory before any revoke. It refuses malformed state or any grant
that the existing planner still authorizes. Every retained grant must be in the
plan. It validates all outgoing host IDs and bounded PEM envelopes before the
first socket request. It does not add a grant format, force policy, or RPC.

The root supervisor must hold its external operator lock, keep korrid and all
other identity/launch writers stopped, and isolate Sunshine's maintenance
service/socket. Run the child with the exact installed certificate-control
environment, service UID/GID, and cleared supplementary groups. The command
uses the production Unix seqpacket adapter and its existing path, mode, peer
credential, frame, and timeout checks. Root must not make these checks permissive.
The command does not prove service or network isolation.

Each exact revoke must succeed before the command deletes its grant and syncs
the grant directory. `changed=false` is success after bulk erase or interrupted
cleanup. The command stops at the first error. Failed and unattempted grants
remain; a verified retry handles durable absence. Success prints only the
reconciled count and `remaining grants: 0`; an empty retry syncs the grant
directory without a socket request. Key, owner event, signed revocations, replay markers, and other
identity files remain unchanged. This reconciles **tracked stale grants only**:
Sunshine's separate producer operation must retire all client pairings, including
untracked clients. Errors or ambiguous outcomes keep all authority isolated;
never restore retired trust. Power-loss durability and live service isolation
still need platform acceptance.

## Event boundary

rust-nostr types stay inside `services/korrid/src/identity.rs`. Other Korri code receives lowercase public-key strings and signed JSON strings.

The identity module can:

- sign and verify NIP-01 events,
- encrypt content with NIP-44 version 2,
- sign the encrypted content as a NIP-01 event,
- verify the event before decryption,
- create an unsigned owner template for an external signer,
- verify that a returned signed event exactly matches its unsigned template and selected person public key,
- apply an owner binding or an owner revocation.

The Android JNI boundary carries only public identity status, unsigned event templates, selected public keys, and complete signed public event JSON. No person private key operation or field exists at this boundary.

The module limits untrusted event sizes before JSON or Base64 processing.

## Security limits

NIP-44 has no forward secrecy. A stolen device key can decrypt recorded messages for that device. Device key rotation is necessary in a later transport slice.

NIP-44 does not hide all metadata. NIP-59 can hide more relay metadata in the relay slice.

A signature proves the event author. The owner statement connects the device key to the person key.

## Peer authorization and person passes

Peer-envelope verification produces one authorization context before RPC dispatch. The context is `OwnerDevice`, `Household`, `Guest`, or `Unknown`. Local browser RPC and the private Unix control listener use their existing local contexts. They do not create peer principals.

The authorization module has one exhaustive policy match over every `RpcRequest` variant. `OwnerDevice` can use every peer action. `Household` and `Guest` can use only scopes in a valid person pass. `stream.launch` covers catalog read, Moonlight resolve, Moonlight launch prepare, session prepare/status/stop/controls, and Moonlight certificate attest/provision. These routes select content from the host catalog and Sunshine application set that the owner installed. The scope does not permit local-app launch, discovery writes, settings, secrets, Moonlight launch cancellation, or certificate revocation.

A person pass is a kind `30079` Nostr addressable event. The event author is the owner of the receiving host. The person's signer signs it outside korrid through NIP-55 or NIP-46. korrid stores or carries only the signed event. The event has empty content and these ordered tags:

```text
["d", "org.korri.person-pass:<32-byte-random-hex>"]
["device", "<device-public-key>"]
["tier", "household" | "guest"]
["expires", "<unix-seconds>"]
["scope", "catalog.read" | "stream.launch"] ...
```

A pass must contain at least one known, non-repeated scope. It expires at the exact `expires` second. Its maximum lifetime is 24 hours from the event timestamp. This bounds offline revocation delay.

Pass revocation uses a NIP-09 kind `5` event from the same person key. It has empty content and exactly these tags:

```text
["e", "<pass-event-id>"]
["k", "30079"]
```

The existing kind `30078` owner statement with `status=revoked` revokes a device. korrid accepts signed revocation events as peer-envelope evidence, then persists them only after the carrying peer request is authorized and its replay nonce is accepted. An unauthorized call performs no host, filesystem, process, settings, or Sunshine effect.

## Authorization persistence and Sunshine reconciliation

Authorization state stays under the established korrid private-state root:

```text
<private-state-root>/identity/authorization-revocations/<event-id>.json
<private-state-root>/identity/peer-certificates/<device-public-key>
```

The revocation file is the complete signed Nostr event. The certificate grant record contains the exact Sunshine host UUID, exact Moonlight client certificate, sender owner statement, and either the owner-device basis or complete signed person pass. This extends the existing `peer-certificates` grant seam; no Sunshine or Moonlight certificate protocol changes.

The authorization module derives a deterministic, device-key-sorted reconciliation plan from the local identity, signed revocations, current time, and persisted grants. The peer router applies each planned item through the existing exact Sunshine certificate revoke adapter and deletes a grant only after Sunshine accepts the revoke. Reconciliation runs after peer authorization and before dispatch. Therefore an unauthorized call cannot cause cleanup effects, and a restart reconciles expired or revoked trust before the next authorized launch.

## Verification

The tests use real temporary directories. They make sure that keys survive a restart and private files keep their modes.

The tests use BIP-340 signature vector 0 from the official Bitcoin BIPs repository. They also use the official NIP-44 version 2 example vector.

The tests reject a changed signed event, a wrong device, a wrong owner, a stale revocation, an invalid key, and a symlinked state root.

## Android owner binding

An unowned Android device publishes its full device public key as the device fingerprint, the requested owner-binding action, one NIP-55 binding URI, and one `Set up owner` action. If no signer is installed, it states that a compatible NIP-55 signer such as Amber is required. The signer lifecycle is explicit: `Unavailable`, `Pending`, `Approved`, `Denied`, `InvalidResponse`, or `Defect`. Only Rust verification can move identity from `Unowned` to `Owned`.

The repository includes the separate `org.korri.signer.test` Android application. It implements the NIP-55 Activity and ContentResolver contracts with configurable approve, deny, delay, malformed, and valid-but-wrong-event behavior. This is the physical proof signer when Amber is absent. Amber interoperability on Bandai remains an external acceptance requirement.

## Relay coordination

`RelayCoordinator` is the only relay-facing product contract. It publishes owner statements, endpoint announcements, and queued coordination commands. It reads same-owner roster evidence and converts private relay events into `EndpointRecord` and `CoordinationCommand` values. It does not import or dispatch product RPC types.

Korri reads every configured relay. It deduplicates by NIP-01 event ID. It publishes to every configured relay. One accepted publish is success. A mixed result is `PublishState::Partial`. Korri has no built-in public relay.

Android and other device settings use the ordered `host.relays` list in `config.yaml`. Linux can supply the same ordered list as a JSON array in `KORRID_RELAYS`. Production URLs must use `wss://`. `ws://` is accepted only for loopback test relays.

The production adapter uses bounded WebSocket connections and supports NIP-42 challenges. The deterministic in-process relay follows NIP-01 replacement and subscription ordering. Both adapters bound event size, read results, stored events, response bytes, subscriptions, and reconnect delay. NIP-11 documents are accepted with unknown fields ignored and are rejected above the local response bound. NIP-65 relay-list events do not override Korri's configured list.

### Same-owner device roster

The roster reuses the signed owner statement above. It adds no event kind, tags, configuration, or persisted format. `DeviceIdentity::derive_owner_statement` extracts a canonical lowercase device key from the strict device tag, checks that it is a secp256k1 public key, then uses the existing owner-statement verifier. A valid signature alone is not enough: kind, empty content, tag order, tag count, address, and status must all match.

`RelayCoordinator::publish_owner_statement` publishes the stored signed owner event unchanged to every configured relay. The EVENT author remains the person key. NIP-42 AUTH uses the device key and the relay's challenge. No person signer call is needed. Publication uses the existing `Published`, `Partial`, and `Failed` results. A device without a stored owner statement cannot publish one. A stored revocation can be published too.

`RelayCoordinator::read_owner_roster` requires a locally owned identity. It queries kind `30078` with exactly one `authors` key: the local owner. This public query omits `#p`. Endpoint, queued-command, and NIP-46 subscriptions retain their recipient `#p` filter. The reader verifies each statement and checks its owner again even when the relay claims to apply the filter. It excludes the local device.

The result is a device-key-sorted `Vec<OwnerRosterEntry>`. Each entry contains `latest` signed evidence and optional `revocation` signed evidence. Each evidence value contains the existing `VerifiedOwnerStatement` and the complete signed event JSON. `latest` follows NIP-01 ordering: newer timestamp, then lower event ID. `VerifiedOwnerStatement::is_newer_than` compares only the same owner/device address.

A signed same-owner device revocation is terminal for peer authorization. Therefore the entry retains the newest observed revocation even if its latest event says `owned`. `OwnerRosterEntry::is_owned` is true only for latest `owned` evidence with no observed revocation. Consumers must retain revocation evidence across reads; a later snapshot without that evidence does not restore membership. A reset creates a new device key.

Each relay read is bounded to 128 events; there are at most eight relays. The reader deduplicates verified event IDs and reconciles all returned evidence before the global cap of 128 devices. Each returned device carries at most two signed events. This order prevents duplicates or newer owned events from hiding a returned device's revocation. The result is a bounded observation, not a complete owner inventory. Absence never proves revocation.

One successful empty relay read is an empty successful roster result, even if other relays fail. If every relay read fails, only the roster boundary returns `RelayError::Unavailable`. Existing endpoint and signer read failure behavior is unchanged. A remembered peer can remain usable while relays are unavailable, but discovery of its revocation is delayed. Bounded reads and relay replacement can also hide evidence; this API does not provide a complete revocation history.

Tests use the in-process relay for replacement, duplicate delivery, caps, and outages. Real loopback WebSocket tests check the public query and rejection of unsolicited wrong-owner evidence. Another checks unchanged person-authored publication with device-authored NIP-42 authentication. Public relays may reject this delegated publication; no public-relay acceptance is claimed.

### Endpoint announcements

A current endpoint announcement is a signed, NIP-44-encrypted NIP-78 kind `30078` addressable event. It is published separately for each known recipient device. Its ordered tags are:

```text
["d", "org.korri.endpoint:<recipient-device-public-key>"]
["p", "<recipient-device-public-key>"]
["expiration", "<unix-seconds>"]
```

The encrypted `EndpointRecord` binds the publishing device key, owner key, positive generation, ordered endpoint candidates, issued time, and expiry. The event author must equal the record's device key. The outer expiration must equal the record expiry. A higher generation replaces a lower generation. At one generation, a later issued time wins.

Relay endpoints are candidate addresses only. They do not create a NAT route. The static configured native peer directory remains a separate `ConfiguredNativePeerDirectory` adapter. Relay traffic never carries interactive RPC, catalog data, artwork, saves, controller input, or stream data.

### Private queued commands

Private queued commands use stored NIP-59 kind `1059` gift wraps with NIP-44 v2 at both layers. The signed seal author is the sending device. The inner rumor uses Korri kind `29100` and carries one bounded tagged command. Both the gift wrap and rumor carry NIP-40 expiry. korrid rejects expired events locally even when a relay retains them.

The first command asks an already-running idle korrid to act. It is not hardware wake. If no process is connected, the relay retains the event until korrid reconnects or the command expires. Owner-binding requests and externally signed owner-binding responses use the same private queue.

### NIP-46 remote signer

The remote signer implements the Korri-owned `PersonSigner` contract. It uses NIP-46 kind `24133` request and response events through the same relay coordinator. korrid generates a separate disposable NIP-46 client key. It never reuses the device identity key.

A connection requests exactly `sign_event:30078`. Every request has a random ID. A response must use the same ID, be signed by the configured remote-signer key, be addressed to the exact client key, fit the response bound, and decrypt as NIP-44 v2. A `sign_event` result must be a valid NIP-01 event from the selected user key and must exactly match the requested owner template.

The protected identity directory adds:

```text
nip46-client.key
nip46.connection.json
```

Both files use mode `0600`. The connection document contains the NIP-46 client public key, remote-signer public key, selected user public key when known, relay URLs from the connection token, and optional one-use connection secret. Public connection data is protected with the client secret because exposing the relationship can still reveal account metadata.

## Deferred work

The next slices must add:

- delivery and device-side installation of externally signed person passes and revocations.

The Linux binary provides `identity status`, `owner-binding-request`, `import`,
and `reset`, plus the narrow offline test-owner retirement above. Linux NIP-46
signer orchestration and general owner transfer remain unimplemented.
