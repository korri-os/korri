---
id: 01M1MT7AFHS742T1GT0R55FEHV
slug: build-the-first-korri-identity-slice-nostr-keys-signed-encry
title: "Build the first Korri identity slice: Nostr keys, signed encrypted RPC, relay coordination"
origin: parked
status: In Progress
priority: high
labels:
  - architecture
  - security
  - identity
  - federation
  - korrid
  - nostr
  - rpc
created: 2026-09-03
source: user
context:
  cwd: korri
  branch: main
  commit: 6c6001ab
  repo: korri
  invoked_by: user
---

# Build the first Korri identity slice: Nostr keys, signed encrypted RPC, relay coordination

## Why it matters

Korri has no identity layer. korrid RPC between devices is plain HTTP with an optional shared bearer capability. The alpha stance is "reachable equals trusted", and nothing ships past alpha without identity. The owner wants no managed auth instance, no Google or Apple sign-in, and user-held credentials. On 2026-09-03 the owner decided the shape of that layer in an interview. This brief records the decisions so that a fresh agent can plan and build the first slice without the conversation. The pairing item 01M1M90CMWX4YK3X924CZ18S4Q is built so that this layer only changes korrid's decision to provision. The mutating-RPC item 01KZFGY0Z4FM1CBRZ2BBEC57V3 is part of this work.

## Acceptance Criteria

- [ ] Each korrid generates one device keypair on first run. The key uses secp256k1 with Schnorr signatures, the Nostr key format. The key survives restarts and config changes.
- [ ] A person identity is a Nostr keypair that the person holds. Korri never stores the person private key. Korri speaks NIP-55 (same device) and NIP-46 (remote) to a signer app. Amber fulfills the signer role today. A Korri signer can replace it later with no protocol change.
- [ ] Every peer RPC travels in one signed and encrypted envelope. The sender device key signs it. The envelope is encrypted to the receiver device key. The same envelope travels over LAN HTTP and over a relay. No second scheme exists.
- [ ] Each envelope carries a timestamp and a nonce. The receiver rejects an envelope outside the accepted clock window. The receiver rejects a repeated nonce inside that window.
- [ ] korrid verifies the signature and looks up the permission of the sender key before it dispatches any tag. Authorization is decidable offline. No account server exists.
- [ ] The code path is identical on LAN, Tailscale, and the open internet. No code checks for Tailscale.
- [ ] The shared bearer capability on /rpc is removed and replaced by per-key permission.
- [ ] Permission tiers exist: owner, household, guest, unknown. A new identity defaults to guest. A guest can launch only software that the owner installed.
- [ ] Every device key carries an owner key. No code assumes that two devices share one owner. Cross-owner hosts are not built in this slice.
- [ ] Two devices with the same owner key trust each other with no ceremony. No PIN, no approval tap, and no pass check exist between them. The owner tier applies to every RPC between them.
- [ ] A device becomes owned through one owner binding: a statement "device key D belongs to owner O" that the person key signs. The device stores it and presents it on first contact with a peer. The peer checks the signature and compares O with its own owner. No lookup and no server exist in that check.
- [ ] The owner binding uses one protocol, a binding request and a binding response, with two transports: LAN discovery and relay. The phone Korri app owns the approval screen and shows the device fingerprint before approval.
- [ ] The Linux host accepts a pre-signed owner binding from its configuration and also prints the relay binding URI in its log. The host needs no screen and no camera.
- [ ] A device with no owner binding announces itself and waits. It accepts no RPC that changes state until the binding exists.
- [ ] A signed owner revocation removes one device from the owner. Peers receive it over LAN or relay. A revoked device gets no trust from the same-owner rule.
- [ ] The host korrid provisions a Sunshine client certificate at once for a device with the same owner key. For any other device, it provisions only when a valid pass with the launch scope exists.
- [ ] When a pass expires, the owner revokes a pass, or the owner revokes a device, the host korrid removes that client certificate from Sunshine.
- [ ] Moonlight and Sunshine certificates stay a separate transport key system that korrid provisions. They are outside the identity key system. The one-curve decision does not apply to them.
- [ ] A device holds a time-limited, scope-limited pass that the person key signed. The device never holds the person key. Revocation deletes one pass. The pass lifetime is short enough that offline revocation is acceptable. State the lifetime in the design.
- [ ] Two devices that share no network exchange endpoint addresses through a Nostr relay as small encrypted events. A sleeping host receives a wake command through the relay. Korri publishes to more than one relay and stores no state that only a relay holds.
- [ ] The stream and every interactive RPC never cross the relay. The relay carries only coordination messages.
- [ ] The recovery rule is written into the design: no reset exists. A lost person key means re-approval of every device and a one-time re-key of that person's data.
- [ ] First-run onboarding generates a key for a person with no key, or connects an existing signer. The flow gets a backup out of the user before the key does any work.
- [ ] A device reset deletes the device key and every pass.
- [ ] All touched gates pass: nix run .#korrid-check regenerates contracts, Android checks pass, and every korrid test passes.

## Related

- `work/items/parking-lot/01KYTR9R4JJDMWBW88DXE3J0QD-design-korri-federated-identity-and-authorization-model-mult.md`
- `work/items/parking-lot/01KZFGY0Z4FM1CBRZ2BBEC57V3-authenticate-mutating-linux-host-rpc-requests.md`
- `work/items/parking-lot/01M1M90CMWX4YK3X924CZ18S4Q-replace-moonlight-pairing-with-korrid-provisioned-certificat.md`
- `work/items/parking-lot/01KZ4J0QNN2B19GZTZK4QGP78R-design-deterministic-plugin-declared-peer-data-sync.md`
- `services/korrid/src/lib.rs`
- `services/korrid/src/upstream_native.rs`
- `services/korrid/src/upstreams.rs`
- `services/korrid/Cargo.toml`

## Notes

## Decisions from the interview on 2026-09-03

These decisions are binding. Do not reopen them without new evidence.

| Question | Decision |
|---|---|
| Person key format | Nostr keypair. secp256k1 with Schnorr signatures. |
| Device key format | Same curve as the person key. One signature library in korrid. |
| Who holds an identity in the first version | Anyone with a Nostr key can be a guest. All four tiers from the July document. |
| Friend devices with their own libraries | Later. Build one owner now. Every device key carries an owner key. No code assumes same owner. |
| Relays | Build relay coordination in this work. Endpoint exchange and wake commands as small signed encrypted events. |
| Tailscale | A pipe that the owner uses. Never a design dependency. No code checks for it. |
| Wire protection | Message-level. Sign and encrypt each RPC envelope to the receiver device key. One envelope on LAN and relay. |
| Person key storage | Protocol first. Korri speaks NIP-55 and NIP-46. Amber fulfills it today. A Korri signer can replace it later. |
| Recovery | Accepted. No reset. Re-approve every device. Re-key that person's data once. |
| Alpha stance | Unchanged. Reachable equals trusted until this layer lands. Nothing ships past alpha without it. |
| Owned devices | Same owner key equals full trust. Automatic pairing with no ceremony. The pass flow exists only for other people. |
| Owner binding | One signed statement per device, through Amber, at device setup. LAN or relay transport. Pre-signed file for the Linux host. |
| Moonlight certificates | Kept as a separate transport key. korrid provisions them. Streaming is a launch action, so the guest tier includes it. |

## Why these decisions

- The owner's three needs: no managed auth instance, no Google or Apple sign-in, user-held credentials. A user-held keypair meets all three. Nostr is one packaging of that model.
- Nostr keys were chosen over plain Ed25519 for one reason: phone signers with approval prompts exist today (Amber). The roaming design in the July document needs that signer. Plain keys force Korri to build it.
- Message-level encryption was chosen because the relay path exists. An envelope that crosses a relay cannot use a live connection scheme. One envelope for every pipe follows the July rule: trust lives in the message, not the pipe. TLS and iroh use other key types and add a second key system.
- Accepted cost of message-level encryption: no forward secrecy. A leaked device key decrypts recorded traffic. Mitigate with scheduled device key rotation. State the schedule in the design.
- Relays are coordination only. A relay cannot carry the stream. Moonlight needs UDP with a direct path. Hole punching and a packet fallback are not in this work. Tailscale stays as the pipe.
- The owner binding is the only ceremony that remains for owned devices. It happens once per device, not once per pair. After it, every owned host and every owned client trust each other on first contact.
- The owner binding has no expiry. A sold or lost device needs a signed revocation. Passes have expiry because they cover other people on your hardware.
- The Linux host is the odd case. It has no screen and no camera. A pre-signed binding in the Nix configuration needs two steps: read the host device key, then sign. The relay URI in the log is the second path.

## Owner binding transports

| Path | How it works | Where it fits |
|---|---|---|
| LAN approval | The new device generates its key and announces "no owner" over mDNS. The phone shows the device and its fingerprint. The owner taps approve. Amber signs the binding. The phone sends it over LAN. | A new handheld in the house. The common case. |
| Relay approval (NIP-46) | The new device shows a URI or QR with its key and a relay. The phone scans it. Amber signs. The phone publishes the binding to the relay. The device reads it. | A device away from home, or with no shared network. |
| Pre-signed file | The owner signs the binding once on the phone. The host configuration carries it as a secret. The host reads it at start. | The Linux host. |
| Same-device signer (NIP-55) | Korri on the phone asks Amber on the same phone. | Only the phone itself. |

## Corrections to the July document

- The July document says that every RPC carries a signature in the "existing (currently unused) headers field of the request envelope". That field belongs to the Effect-RPC envelope in services/korrid/src/upstream.rs. That envelope dies at switchover. The native tagged /rpc wire (RpcRequest in services/korrid/src/lib.rs, tag field _tag) has no headers field. The builder must design the signed envelope for the native wire. Do not grow the Effect-RPC envelope.
- The July document names installControlSecret. The current code uses rpc_capability, a Bearer token on the Authorization header in services/korrid/src/lib.rs around line 2461. That is the shared secret to remove.

## What Nostr supplies and what Korri owns

| Concern | Source |
|---|---|
| Key format, signature algorithm, key backup format (NIP-49) | Nostr |
| Signer protocol (NIP-55, NIP-46) | Nostr |
| Relay transport for coordination events | Nostr |
| Encryption primitive for envelopes | Reuse the NIP-44 construction with device keys, or justify another. Decide from the crate landscape. |
| RPC envelope shape, replay rules, clock window | Korri |
| Authorization: tiers, passes, permission lookup | Korri |
| Conflict rules for any synced data | Korri. Never latest-event-wins. |

## Open questions for the builder

Settle these from real code and real crates. Do not invent.

- Which Rust crate provides secp256k1 Schnorr and the NIP-44 construction. Check what services/korrid/Cargo.toml already pulls in through reqwest and rustls.
- How the Android shell reaches Amber through NIP-55 and hands the result to the embedded korrid cdylib.
- How korrid on the Linux host receives a pass when no person key exists on that host. The host never needs the person key. It needs the signed pass.
- How the envelope wraps RpcRequest and RpcResponse without a second serialization path. The generated contracts in contracts/generated are read-only.
- Which relays Korri publishes to by default, and how the owner changes that list.
- Accepted clock skew and the behavior of a device with a bad clock.
- Pass lifetime and device key rotation schedule.
- How the pairing item 01M1M90CMWX4YK3X924CZ18S4Q changes its provisioning check from "reachable" to "same owner key, or a valid pass with the launch scope". Nothing below that check moves.
- How the host korrid learns that a pass expired or a device was revoked, so that it removes the Sunshine client certificate. The pairing item has a minimal revocation path. Connect the two.
- The exact shape of the owner binding statement and the owner revocation statement. Ground on Nostr event conventions where they fit. Do not invent a persisted schema without a producer.
- How the phone Korri app hears a "no owner" announcement on LAN, and how the Linux host prints its relay binding URI.

## Gaps that stay open after this slice

- Hole punching and a packet fallback path. Tailscale covers this today.
- Cross-owner hosts. The owner key on every device keeps this open.
- Per-person data separation on Android and guest data wipe. The July document names this. It needs its own slice.
- Save roaming keyed by the person public key. See the sync item 01KZ4J0QNN2B19GZTZK4QGP78R.

## Guards

- Schema guard from AGENTS.md applies. Do not invent persisted schema, file names, or field names. Ground on existing producers or on the cited NIPs.
- Peer korrids speak the native tagged /rpc wire only.
- Do not push, publish, or activate a device without approval.
- Nostr protocol facts in this brief come from the agent's training knowledge. The builder must read the cited NIPs before it depends on them.
