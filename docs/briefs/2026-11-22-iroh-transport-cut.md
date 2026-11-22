# ADR 0095 transport cut

## Identity and authorization

The Nostr device key is secp256k1. iroh requires an Ed25519 EndpointId.
`identity/iroh.key` remains the separate, persisted transport key.

The existing encrypted, device-signed kind-30078 endpoint record now carries
`endpointId`. Its name and string encoding come from the former reach `Offer`.
The record retains `devicePublicKey`, `ownerPublicKey`, `generation` and
`issuedAt` from its existing producer. Addresses, expiry and viewer metadata
are removed. The record is published on startup, on a transport-key change,
and to a newly learned member. Time alone never renews the record.

The parent clarified the first-contact boundary during implementation:

- An incoming connection can carry signed peer HTTP before its caller appears
  in the receiving device's records.
- The caller presents its own encrypted, device-signed endpoint event in the
  `x-korri-endpoint` HTTP header. Relay publication and this proof share one
  record producer and the existing generation/timestamp reservation.
- The existing peer signature and owner/vouch authorization must pass. The
  endpoint event must name that device and owner, and its `endpointId` must
  equal the connection's authenticated `remote_id`. An RPC signature alone
  proves no transport binding. A replay from another EndpointId is rejected
  before consuming the RPC nonce.
- Valid proof adds the signed membership statement and endpoint event to the
  directory, then binds the connection. A superseded endpoint binding cannot
  replace the directory's newer key. Both Sunshine and netplay remain closed
  until this proof succeeds.
- A signed endpoint record can establish the binding before the first request.
- Sunshine, UDP and netplay channels reject an unbound connection. Membership
  revocation closes a bound connection; it does not start another connection.

Both directions use the cached connection, including accepted connections.
A single datagram reader separates Sunshine requests from replies. Stream
forwards subscribe to replies; they do not compete with the Sunshine reader.

The explicit legacy Effect-RPC adapter in `upstream.rs` is unchanged. Per
`AGENTS.md` and the parent's scope decision, it remains only for aka until
aka's separate switchover. It gains no iroh wrapper or new identity schema.

## Endpoint and ports

Production uses `Endpoint::builder(iroh::endpoint::presets::N0)` plus
`iroh_mdns_address_lookup::MdnsAddressLookup::builder()`. iroh 1.4.1's preset
installs n0's relays, pkarr publisher/resolver and DNS resolver. The existing
`tls-ring` feature supplies the crypto provider. The companion lookup crate
is pinned to 0.6.1, whose iroh dependency accepts 1.4.1.

Linux uses the existing `KORRID_ADDRESS` port for iroh UDP. This lets the
firewall admit offline LAN dials without exposing all ephemeral UDP ports.
The product opens UDP 39217 and multicast UDP 5353. It no longer exposes TCP
39217. The HTTP listener binds loopback for the local shell; peer HTTP travels
inside iroh. Android uses an ephemeral UDP port and has no Nix firewall.

Handshakes have 64 slots and a 10 s deadline; a completed handshake releases
its slot. Unbound connections have a separate 8-slot budget and must prove
their endpoint within 10 s or close. Established members have 64 separate
slots, so idle strangers cannot fill them. Valid proof releases the unbound
slot immediately.

Android's netwatch 0.19.3 monitor does not emit network changes. A small Rust
JNI bridge reads the active network handle and link addresses every 2 s.
Only a change calls `Endpoint::network_change()`. It has no candidate list,
metering rule or path choice. Its task is joined at embedded-server shutdown.

A connection loss is terminal for the stream session that used it, not for the
peer. The closed connection is evicted. A later RPC, save request or new stream
dials the peer by key on demand. Concurrent callers share the per-peer dial
lock and one new connection; there is no background retry loop. The old session
keeps its old lease and reports failure once. Its status watcher does not dial
again. Netplay polls only its captured connection, with no dial operation,
and does not replace its forward or relaunch RetroArch after loss.

## One-off deployment migration

Do **not** remove `identity/iroh.key`, owner statements, saves or publication
counters. The persisted directory document is unchanged, but its old encrypted
`endpointEvent` values contain the removed address schema. Old values must not
be read as the new binding schema.

Before deploying this cut to a device, stop korrid. Make a private backup of
`<privateStateRoot>/federation/peers.json`. In that document, set each existing
`peers[device].endpointEvent` to JSON null. Keep every other field unchanged.
Write the file atomically with its existing owner and mode 0600. This operation
uses the existing `RememberedPeer.endpoint_event` nullable field; it creates no
migration field or fallback reader. New signed binding records repopulate it.

Remove an obsolete `host.stun` entry from any mutable `device.yaml`, with a
backup first. Product Nix configuration no longer supplies `KORRID_STUN`.
Restart korrid only after these one-off operations and the new deployment.
This run does not deploy or modify device data.

The federation proof holder now accepts `ROOT PORT RELAYS`, with no external
Moonlight address. Its Sunshine must run on the holder's loopback at the
standard ports. The Android proof callers use that same boundary.

## Proofs

Focused Rust tests exercise signed RPC with key-only dialing, TCP/UDP stream
forwards, netplay forwards, in-band first-contact authorization and terminal
connection loss. The root-only two-path test runs on aka in isolated network
namespaces. It removes the selected link and verifies another HTTP request on
the same iroh connection. It is ignored in ordinary sandboxed test runs.

Remaining operational costs: n0 sees endpoint availability, LAN discovery
advertises the iroh identity, and relayed streaming depends on n0 capacity.
Android multicast delivery still needs a real-device check; an APK build alone
does not prove offline LAN discovery on Android.
