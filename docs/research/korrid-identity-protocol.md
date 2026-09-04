# Korri identity protocol

Status: accepted for the first identity slice

Source work item: `01M1MT7AFHS742T1GT0R55FEHV`

Protocol source revision: `nostr-protocol/nips@6aeea6093786644e892dd2869fa5b642fddd271d`

## Decision

Korri uses Nostr-format keys for person identity and device identity. Each key uses secp256k1 and BIP-340 Schnorr signatures. Korri does not need a managed account service.

The person controls the person key. Korri does not store that private key. An Android signer supplies signatures through NIP-55. A remote signer supplies signatures through NIP-46.

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
- apply an owner binding or an owner revocation.

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

## Deferred work

The next slices must add:

- relay coordination with NIP-59 wrapping,
- NIP-55 and NIP-46 adapters on Android,
- delivery and device-side installation of externally signed person passes and revocations.
