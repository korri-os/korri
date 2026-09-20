# Decide automatic Nostr identity and later replacement

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: resolved
Blocked by: 01

## Question

How can first use create a real Nostr user identity without mandatory sign-in, while allowing safe backup and an explicit switch to another identity later?

The accepted product direction is recorded under [User identity](01-define-intended-product.md#user-identity). Resolve the remaining lifecycle and security decisions here, not the product direction again.

Read the accepted [identity protocol](../../../docs/research/korrid-identity-protocol.md), the actual [device identity implementation](../../../services/korrid/src/identity.rs), the [person signer contract](../../../services/korrid/src/remote_signer.rs), and the [restricted test-owner replacement](../../../services/korrid/src/identity/offline.rs). Current person and device identities already use Nostr-format keys. The accepted protocol keeps the person's private key outside Korri and assigns generation and backup to the signer. Normal owner imports refuse changes to a different owner. The offline exception only retires the known test owner; it is not a general transfer feature.

Resolve with Simon who generates and holds the automatically created user key, what backup and recovery mean, and what consent is needed when switching identities. Any change to existing key custody or ownership security needs explicit approval. Keep device identity, active user identity, and device ownership distinct. Changing the active user must not silently transfer device authority.

The desired switch offers a choice to transfer local library choices, saves, and preferences without reinstalling. Define how that choice preserves or separates existing data and what happens to old access. Do not promise that another identity inherits old signatures or that other devices automatically trust it. Decide the safe behavior for failed or interrupted replacement before claiming a complete design.

Ground any storage or protocol proposal in the existing producer/consumer contracts or an explicit user choice. This ticket records decisions, not a new signer implementation or a device operation. Cross-device save synchronization remains deferred by the parent product decision; an explicit local data transfer does not authorize building synchronization.

## Comments

2026-09-20. Grilled with Simon in three rounds through the ask tool. Facts checked in source before each round: `services/korrid/src/identity.rs` (owner binding accepts a newer statement only from the same owner; `reset` removes the identity directory and makes a new device key), `services/korrid/src/lib.rs:1469,1500,1630` and `authorization.rs:134` (local portal calls use the device owner as the person), `services/korrid/src/host/play_log.rs` (the only person-keyed record today), `services/korrid/src/launcher/linux_plugin.rs:79` (plugin saves use a fixed `users/default` account root), `services/korrid/src/remote_signer.rs` (`PersonSigner` contract and unwired `Nip46PersonSigner`), `services/korrid/src/identity_cli.rs` and `main.rs:500` (Linux commands are `status`, `owner-binding-request`, `import`, `reset`, plus the two offline commands). NIP-55 was Android only, and Android is gone.

Simon added one boundary after the last round: save files and save states are complicated and are deferred. The transfer decision below therefore covers only the play log and future unsigned per-person records; save handling is fog.

## Answer

Resolved 2026-09-20. All choices are Simon's, selected through the ask tool.

### Terms

- **Person key**: the Nostr key that identifies a person. Korri never holds its private half in korrid.
- **Local signer**: a separate service on the device, under its own UID and private directory, that holds a person key and answers korrid through the existing `PersonSigner` contract.
- **Automatic identity**: the person key the local signer creates on first boot, with no sign-in.
- **Identity switch**: the explicit, consented replacement of the device owner by a different person key.

### Custody

The local signer holds the automatic identity. korrid never holds a person private key; the rule from the accepted protocol stays. The signer is a new service. The transport between korrid and the signer is an implementation choice grounded in the `PersonSigner` trait; this ticket does not select it.

### First boot

The local signer creates the automatic identity, and korrid binds it as device owner at once through the existing owner-binding path. No screen, no sign-in. Nothing is published to any relay, and no federation is joined. The user learns about the identity in settings later.

### Backup and recovery

Backup is a NIP-49 encrypted export of the person key, on demand from settings, shown as text and QR, with a plain statement that Korri cannot recover a lost key. Recovery is importing that export into a signer; it is the identity switch, run on a fresh device against its automatic identity. Korri has no account recovery. A key that was never exported is lost with the device.

### Two fresh devices

Two fresh devices make two different automatic identities. They are two persons until the user runs the identity switch on one with the other's backup, or points both at one NIP-46 remote signer. First boot offers no "use an existing identity" branch.

### Identity switch

The switch is an owner replacement, not a user layer above a permanent owner. It is the only way device authority moves, and it never moves silently.

The replacement identity comes from a NIP-49 backup imported into the local signer, or from a NIP-46 remote signer.

Consent before commit: the new key must sign the new owner binding, and the user must confirm a plain list of what is lost. There is no forced export of the old key first.

What is lost: every peer pairing and every stream client trust, as in the 2026-09-06 offline cutover. The old key keeps its signed history; the new key inherits none of it, and no other device trusts the new key without its own binding.

Order: prepare, then commit. Stage a new device key, get the new owner binding signed for it, get the old owner's revocation signed by the local signer. Then one atomic swap of the identity directory. Any failure before the swap leaves the old identity untouched. Interrupted staging leaves only unused private temporary files.

Relay publication: the switch publishes the old owner's revocation only if the old owner statement was published before. A never-published identity leaves nothing on any relay.

### Local data

Transfer, when the user chooses it, re-keys unsigned per-person records from the old key to the new key. After the switch, the device has one owner of the data; the old key's records cease to exist locally. Signed events stay with the key that signed them; nothing is re-signed.

No transfer, when the user declines: the old key's local records are deleted as part of the switch. There is no orphan state and no fallback read.

Today the only person-keyed record is the play log at `<root>/<personPubkey>/<gameId>.json`. Library choices and preferences per person are a discussion brief, not code. Save files and save states are deferred by Simon's note; this answer does not decide their transfer, location, or ownership. The fixed `users/default` account root that plugins use today is therefore not changed by this decision.

Resume: if power fails after the commit and before the transfer completes, korrid finishes the transfer from its journal at next start, before it serves any per-person data. This is the same shape as the plugin uninstall resume already decided.

### Old key and device reset

After the switch, the old automatic key stays in the local signer as retired and exportable until the user deletes it. It has no device authority. Deleting it is a separate explicit action.

`korrid identity reset` stays a device-identity reset: it removes the device key and owner event only. The local signer keeps its keys. A full wipe of the signer is a separate explicit operation.

### Costs and limits

- One new service, its own private state, and a korrid-to-signer transport to build and verify.
- Every identity switch costs re-pairing on every peer and stream client.
- No recovery without a prior export. This is the price of no managed accounts.
- Two devices are two persons until the user unifies them by hand.
- Save transfer is undecided; a user who switches identity today keeps saves only because they sit under `users/default`, which is not a designed outcome.
- Nothing here is verified runtime behavior. The NIP-46 signer is unwired, no local signer exists, and no CLI or RPC performs a switch.

This answer records decisions. It authorizes no schema beyond the existing identity, play log, and signer files, no key-custody exception, no device write, and no deployment.
