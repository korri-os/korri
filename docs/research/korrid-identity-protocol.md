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

A new device accepts `owned` as its first owner statement. An owned device accepts a newer statement only from the same owner. A revoked device accepts no owner changes. A reset is necessary before another person can own it.

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

`RelayCoordinator` is the only relay-facing product contract. It publishes endpoint announcements and queued coordination commands. It converts relay events into `EndpointRecord` and `CoordinationCommand` values. It does not import or dispatch product RPC types.

Korri reads every configured relay. It deduplicates by NIP-01 event ID. It publishes to every configured relay. One accepted publish is success. A mixed result is `PublishState::Partial`. Korri has no built-in public relay.

Android and other device settings use the ordered `host.relays` list in `config.yaml`. Linux can supply the same ordered list as a JSON array in `KORRID_RELAYS`. Production URLs must use `wss://`. `ws://` is accepted only for loopback test relays.

The production adapter uses bounded WebSocket connections and supports NIP-42 challenges. The deterministic in-process relay follows NIP-01 replacement and subscription ordering. Both adapters bound event size, read results, stored events, response bytes, subscriptions, and reconnect delay. NIP-11 documents are accepted with unknown fields ignored and are rejected above the local response bound. NIP-65 relay-list events do not override Korri's configured list.

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

The current environment-driven Linux binary does not have a command framework. The owner-binding import/status CLI stays with the later Nix host-binding slice rather than broadening that binary in this Android slice.
