# Federation restoration brief

Status: aligned with the owner on 2026-09-04. Ready for implementation.

Audience: an implementation agent that starts with no prior context.

## Purpose

Korri `main` has a secure, static, authenticated federation. Korri `legacy` had an open, automatic, unauthenticated federation. This brief lists the delta, the agreed scope, and two implementation bundles. Implement Bundle A first. Then implement Bundle B.

Read `AGENTS.md` first. `legacy` is read-only reference material. Do not copy legacy code. Harvest behavior, then write it in the `main` architecture.

## Repository facts

- Repository: `/home/simonwjackson/code/sandbox/korri`
- Target branch: `main`
- Korrid service: `services/korrid/src`
- Peer RPC surface: `RpcRequest` in `services/korrid/src/lib.rs` near line 1009
- Peer envelope security: `services/korrid/src/peer_rpc.rs`
- Authorization policy: `services/korrid/src/authorization.rs`
- Peer registry: `services/korrid/src/upstreams.rs`
- Native peer client: `services/korrid/src/upstream_native.rs`
- Legacy HTTP peer client: `services/korrid/src/upstream.rs`
- Relay coordination: `services/korrid/src/relay.rs`
- Identity: `services/korrid/src/identity.rs`
- Linux host session control: `services/korrid/src/host/session_state.rs`
- Linux host launch units: `services/korrid/src/host/systemd_unit.rs`
- Android shell: `clients/android/app/src/main/java/com/limelight/KorriShellActivity.java`
- Android brain: `clients/android/app/src/main/java/com/simonwjackson/korri/korrid/KorriBrainService.java`
- Bridge contract: `contracts/bridge/korri-native-bridge.ts`
- Identity protocol: `docs/research/korrid-identity-protocol.md`
- Full gate: `nix run .#korrid-check`

## Test devices

- Bandai: Android client and brain. ADB `100.91.213.4:42757`. Debug package `com.simonwjackson.korri.debug`.
- Zao: NixOS Linux host. korrid on `0.0.0.0:39217`. Sunshine on `zao:47989`.
- Owner key: `f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9`
- Zao device key: `7bb368b270acb72d81856b7b7010d919ec4882afe7c3aaa56b7b6839e46b47f6`
- Zao relays: `wss://relay.nostr.band`, `wss://relay.primal.net`
- Bandai peer config: `/sdcard/korri/upstreams.json`
- Zao deployment is in the Mountainous repository: `/home/simonwjackson/code/github/simonwjackson/mountainous`, branch `unified`, host `hosts/zao/default.nix`.
- Activate NixOS candidates in `test` mode before you change the boot default.
- Keep device identity state separate from package and bundle rollback.

## Federation delta

### Legacy behavior

Legacy federation lived in `product/apps/portal/peers/*` and `product/platform/stream/*` on the `legacy` branch.

| Area | Legacy |
|---|---|
| Discovery | mDNS browse for `_korri-stream._tcp` with `caps: "source"` |
| Advertisement | `avahi-publisher.ts` and a fallback publisher |
| Peer memory | `peer-store.ts` keyed by `hostId`, reloaded on boot |
| Gossip | Planned, not finished |
| Identity | `hostId` string, `identityVerified: false` |
| Trust | None. Any LAN peer was accepted |
| Transport | Plain HTTP RPC |
| Catalog | Fan-out to `app.source.list` per peer, source-tagged, folded by identity |
| Source status | `sourceStatus()` with `catalog` and `streamControl` flags |
| Play stats | Carried per peer entry |
| Remote session | `app.session.status`, `stop`, `freeze`, `thaw` on the source host |
| Remote content | Plugin install proxy, catalog search, acquisition |
| Moonlight pairing | Manual shell script |

### Main behavior

Main federation lives in Rust korrid.

| Area | Main |
|---|---|
| Discovery | Static `upstreams.json` or `KORRID_UPSTREAMS` only |
| Advertisement | None |
| Peer memory | None |
| Identity | secp256k1 device key plus owner binding |
| Trust | Same-owner automatic. Household and guest use scoped passes |
| Transport | Signed and NIP-44 encrypted envelopes with replay protection |
| Catalog | Parallel fan-out with timeout. `failures` list. No folding |
| Source status | `Health` only |
| Play stats | None |
| Remote session | `prepare_stream` only. Status and stop fail for native peers |
| Remote content | None |
| Moonlight pairing | Automatic certificate provisioning |
| Relay | Endpoint announcements and gift-wrapped commands exist, but the registry does not use them |

## Missing list and disposition

| # | Item | Disposition |
|---|---|---|
| 1 | Automatic peer discovery | Bundle B, through relays |
| 2 | mDNS advertisement | Dropped. Relays replace it |
| 3 | Durable peer memory | Bundle B |
| 4 | Peer gossip | Bundle B, replaced by relay roster |
| 5 | Live peer state | Bundle B, korrid state and RPC only |
| 6 | Federated release folding | Dropped for now |
| 7 | Remote plugin install | Dropped. Needs the plugin host, which is not on `main` |
| 8 | Remote catalog search and acquisition | Dropped. Needs the acquisition platform, which is not on `main` |
| 9 | Remote session status and stop | Bundle A |
| 10 | Peer data sync | Dropped for now |
| 11 | Desktop upstream picker | Dropped. No desktop client on `main` |
| 12 | Remote source status | Bundle A |
| 13 | Remote freeze and thaw | Bundle A, with new host-side freeze |
| 14 | Remote overlay stop | Bundle A |
| 15 | Peer play stats | Bundle A, with new host-side play log |
| 16 | Structured entry source | Bundle A |
| 17 | Peer UI | Dropped. No UI work in these bundles |
| 18 | Relay endpoint directory not wired | Bundle B |

## Agreed constraints

- No UI work. RPC, bridge events, and host behavior only.
- No mDNS. Nostr relays are the discovery transport.
- No plugin host, acquisition platform, folding, or data sync.
- Use ASD-STE100 Simplified Technical English in docs and commit bodies.
- Every new peer route goes through the signed envelope and the exhaustive authorization match. No plaintext path.
- Relays carry coordination only. They never carry catalog data, artwork, saves, input, RPC, or stream data.
- Keep the legacy HTTP peer client working where configured.
- Validate on Bandai and Zao before you land.

## Bundle A: Secure peer session control and play stats

Goal: a secure native peer can fully control a remote session, every catalog entry names its source device, and each peer reports play stats.

Backlog item: `work/items/parking-lot/01M1NGVGXPBTHFRDGZHNJQ1HYP-route-remote-session-status-and-stop-through-secure-native-p.md`

### A1. Session status and stop through the selected native peer

- `UpstreamRegistry::session_status` and `session_stop` in `upstreams.rs` call `legacy_host()`. This returns `no legacy session upstream configured` when only native peers exist.
- Remember which peer served `prepare_stream`. Route status and stop to that peer.
- Stop carries the expected launch identity. Reject a stale identity.
- Force stop requires owner-device authorization.
- Keep the legacy route when a legacy host is configured.

### A2. Host-side freeze and thaw

- `HostSessionControl` in `host/session_state.rs` has `prepare`, `status`, and `stop`. Add `freeze` and `thaw`.
- Use the cgroup v2 freezer on the launch unit that `systemd_unit.rs` owns. Do not send signals to the game process directly.
- Status reports `frozen` as a distinct state.
- Freeze is idempotent. Thaw of a session that is not frozen is a no-op with a typed result.

### A3. Freeze and thaw over peer RPC

- Add `SessionFreeze` and `SessionThaw` to `RpcRequest`.
- Add both to the exhaustive policy match in `authorization.rs`. Include them in the `stream.launch` scope, next to session stop.
- Add both to the native client and the Linux host router.
- Route them through the same selected peer as A1.

### A4. Overlay remote control

- The Android gameplay overlay calls stop, freeze, and thaw through the local korrid brain.
- Each call has a bounded timeout. Legacy used 10 seconds for stop.
- A timeout returns a typed failure to the overlay. It does not hang the overlay.

### A5. Source status RPC

- Add `SourceStatus` to `RpcRequest`.
- Result has `catalog: available | unavailable` and `streamControl: enabled | disabled`.
- `streamControl` is `disabled` when Sunshine is not active or the certificate broker is unavailable.
- Authorization: same as catalog read.

### A6. Structured source on catalog games

- Add `source` to `Game` in `lib.rs`: `devicePublicKey: Option<String>`, `label: String`, `isLocal: bool`.
- The local catalog sets `isLocal: true` and the local device key.
- The registry sets the peer label and expected device key on each remote game.
- Keep the current `host` field and label prefix until the UI reads `source`. Do not remove them in this bundle.
- Update `typeshare` output and the bridge contract.

### A7. Play-log recording and play stats

- Record one play-log entry per session on the host: game id, launch id, start time, end time.
- Store the log in the private state root. Use the same atomic write pattern as the identity module.
- Derive `playStats` per game: `lastPlayed`, `playCount`, `totalPlaytimeSeconds`.
- Add `playStats: Option<PlayStats>` to `Game`. The local catalog and each peer catalog carry it.
- Legacy sources for behavior only: `product/platform/library/play-log-store.ts`, `product/platform/library/play-stats.ts`.

### Bundle A verification

- Rust tests: native-peer status, normal stop, stale identity rejection, force-stop authorization, freeze, thaw, frozen status, play-log write and read, play-stats derivation, `SourceStatus` both states, `source` on local and remote games.
- Device: stream Bandai to Zao. From the overlay, read status, freeze, thaw, and stop. Confirm the Zao game unit stops.
- Device: stop Sunshine on Zao. `SourceStatus` reports `streamControl: disabled`.
- Device: Bandai catalog shows Zao games with Zao's device key as `source.devicePublicKey` and `isLocal: false`.
- Device: after one play on Zao, the Bandai catalog shows Zao's `playStats` for that game.
- Full gate: `nix run .#korrid-check`.

## Bundle B: Relay discovery and peer memory

Goal: owned devices find each other through relays without a hand-written peer list, remember each other on disk, and report a live state.

Backlog items: `work/items/parking-lot/01KYTECFJ2BARWPV834DRFTEGX-spike-mdns-discovery-on-android-before-building-peer-discove.md`. Close the mDNS spike as superseded by this brief.

Depends on Bundle A for the `source` object.

### B1. Same-owner device roster on relays

- Relay endpoint announcements are addressed to one recipient device key. A device can only announce to devices it knows. Bundle B adds the roster.
- Each device publishes its signed owner-binding event to the configured relays as a public addressable event. The event already exists in `owner.event.json`.
- Each device queries the relays for owner-binding events that name the same owner key.
- Verify each binding with the existing rules in `identity.rs` before you accept it. Reject unknown owners, revoked devices, and malformed events.
- The verified set is the roster.

### B2. Endpoint announcement to the roster

- On start and on address change, publish one endpoint announcement to each roster device. Use the existing `publish_endpoint` path in `relay.rs`.
- Candidates are ordered. Include the LAN hostname, LAN address, and any overlay name such as Tailscale MagicDNS when present. Tailscale is optional. Do not depend on it.
- Respect the existing bounds: `MAX_ENDPOINT_CANDIDATES`, `MAX_ENDPOINT_LIFETIME_SECONDS`.

### B3. Wire the relay directory into the registry

- `relay.rs` has `PeerDirectory`, `ConfiguredNativePeerDirectory`, and `RelayEndpointDirectory`. `upstreams.rs` never calls `endpoint_candidates`.
- `UpstreamRegistry` resolves each roster device key to endpoint candidates. Try candidates in order with a bounded timeout each.
- Static `upstreams.json` stays as a manual override and a fallback. When the file exists, its entries win for the same device key.
- Every resolved peer still requires the expected device key. The key comes from the roster, not from the endpoint.

### B4. Durable peer memory

- Store known peers in the private state root, keyed by device public key: label, last known candidates, owner key, first seen, last seen.
- Reload on start. A device that has met a peer once can reach it at home without internet.
- Refresh the stored candidates when a newer endpoint record arrives.
- Never store the local device. Never store private keys or pass secrets.
- Use the same `0700` directory and `0600` file modes as the identity module.

### B5. Live peer state

- Track each peer as `loading`, `ready`, or `failed` with a timestamp and last error.
- Update the state from roster queries, endpoint resolution, and catalog fetches.
- Add a `PeerList` RPC that returns the state for the local caller. Owner-device authorization only.
- Emit one bridge event to the Android shell when the set changes. No Shift UI.

### B6. Android configuration

- The Android brain reads relays from `host.relays` in `config.yaml`. Confirm Bandai has the same relays as Zao.
- `upstreams.json` becomes optional. The render script in `services/korrid/deploy/render-upstreams-android.sh` stays for manual override.

### Bundle B verification

- Rust tests with `InProcessRelayNetwork`: roster from same-owner bindings, rejection of other-owner bindings, rejection of revoked devices, endpoint resolution order, static override wins, peer memory write and reload, state transitions.
- Device: remove `/sdcard/korri/upstreams.json` from Bandai. Bandai finds Zao through the relays and shows the Zao catalog.
- Device: reboot Bandai with Wi-Fi on and internet blocked. Bandai reaches Zao from stored candidates.
- Device: change Zao's korrid port in Mountainous, deploy in `test` mode. Bandai reaches Zao through the new endpoint record without a config change. Roll back the port.
- Device: stop korrid on Zao. `PeerList` shows Zao as `failed`. Start korrid. `PeerList` shows Zao as `ready`.
- Device: verify partial-relay operation when one relay is unreachable.
- Full gate: `nix run .#korrid-check`.

## Open questions for the implementer

- A2: confirm the launch unit cgroup path is writable by korrid under the current hardened service. If not, add a small privileged helper in the NixOS module, not a broader capability.
- B1: the public owner-binding event exposes the owner key and device key on public relays. This was accepted for the first slice. Record the decision in `docs/research/korrid-identity-protocol.md` when you land B1.
- B4: decide whether a peer that has not been seen for a long time is dropped or kept. Legacy kept peers indefinitely. Keep them unless the owner binding is revoked.

## Out of scope

Do not implement in these bundles: mDNS, Shift UI, folding, plugin host, acquisition, data sync, desktop client, cross-owner federation, per-person Android data isolation, guest cleanup.

## Landing rules

- One branch per bundle. Rebase onto `main` before you land.
- Fast-forward `main`. Do not squash-merge.
- Update Mountainous `unified` for Zao host changes.
- Remove worktrees and branches when the bundle is landed.
- Update this brief's status line when a bundle lands.
