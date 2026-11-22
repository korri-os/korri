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

## Last-seen iroh addresses

iroh 1.4.1 provides `address_lookup::memory::MemoryLookup` with
`from_endpoint_info`, `set_endpoint_info` and `remove_endpoint_info`.
`AddressLookupServices` runs its providers in parallel, so memory can sit next
to n0 and mDNS. `Endpoint::remote_info` reports known addresses, including
inactive ones; `Connection::paths_stream` reports path changes. iroh's remote
memory is temporary. Its native `EndpointAddr` has Serde support, but this
version supplies no disk persistence for it. These are the APIs in
`iroh/src/address_lookup/memory.rs`, `iroh/src/address_lookup.rs`,
`iroh/src/endpoint.rs` and `iroh-base/src/endpoint_addr.rs`.

The owner approved a disposable cache at
`<privateStateRoot>/federation/iroh-addresses.json`, separate from `peers.json`.
Its JSON is a list of iroh's serialized `EndpointAddr` values, with no Korri
address schema. Authenticated live connections fill it on binding, path
updates and the existing membership-check tick. At startup it populates a
`MemoryLookup` added alongside the existing n0 and mDNS providers. Calls still
dial only by key. Korri never examines address values or transport kinds,
orders them, or names a network. Only the first 16 values in iroh's reporting
order are retained to enforce the count bound. Roster removal removes the
peer from both the file and this lookup under the same authority lock.

The cache is private (0600), atomically replaced, limited to the roster and
an 8 MiB file bound. Missing, corrupt, unsafe or oversized files are ignored
and rebuilt; cache errors never invalidate federation authority. Cached hints
do not authorize a peer: iroh authenticates the endpoint key and the existing
signed membership/endpoint proof still controls channels. Stale addresses may
not work, and a restart must retain a reachable address and UDP port for an
offline dial to succeed. No retry loop or stream recovery is added.

## One-off deployment migration

Do **not** remove `identity/iroh.key`, owner statements, saves or publication
counters. The persisted directory document is unchanged, but its old encrypted
`endpointEvent` values contain the removed address schema. Old values must not
be read as the new binding schema.

This migration applies to a pre-ADR-0095 bundle. The address-cache addition
needs no authority migration. Stage and validate the new system and bundle
before starting. Use this order:

1. Switch the system first. This may restart the **old** korrid.
2. Stop korrid after that switch. Make a private backup of
   `<privateStateRoot>/federation/peers.json`. With a backup first, remove any
   obsolete mutable `host.stun` entry from `device.yaml` now; product Nix no
   longer supplies `KORRID_STUN`.
3. Set each existing `peers[device].endpointEvent` to JSON null. Keep every
   other field unchanged. Write atomically with its existing owner and mode
   0600.
4. Switch the bundle immediately and start only the new korrid. Do not run or
   restart the old korrid between clearing the events and switching the bundle.

Complete steps 2–4 as one maintenance operation. If bundle activation cannot
complete, leave korrid stopped instead of restarting the old bundle.
`korri-bundle-select switch` restores and restarts the old bundle when the new
one fails (`services/inputd/src/bundle_select.rs`), so it cannot keep that
promise. Where the device allows the downtime, use
`korri-bundle-select offline-select <expected-current> <new-bundle>
--acknowledge-exclusive-quiescence` with every Korri consumer stopped, then
start the Korri services. A failed start then leaves the old korrid stopped.
With `switch`, a failed start brings the old korrid back; clear the events
again before the next attempt. On ibuki,
clearing the events before the system switch let the restarted old korrid write five old-format
values back; the new korrid then refused startup with
`open federation authority: Identity`.

This operation uses the existing nullable `RememberedPeer.endpoint_event`
field; it creates no migration field or fallback reader. New signed binding
records repopulate it. This run does not deploy or modify device data.

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
