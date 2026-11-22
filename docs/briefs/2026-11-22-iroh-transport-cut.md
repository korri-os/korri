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
- The existing peer signature, owner/vouch authorization and replay checks
  must pass before the connection gains stream authority.
- That proof adds the existing signed membership statement to the roster and
  binds the verified device key to iroh's authenticated remote EndpointId on
  that same connection. The connection cache stores this in-band binding.
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

A connection loss is terminal for that connection. The cache retains the
closed connection so status polls cannot start a reconnect loop. A new daemon
lifetime starts with an empty connection cache. The stream watcher reports a
terminal failure once per selected launch. Netplay does not replace its forward
or relaunch RetroArch after loss.

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
