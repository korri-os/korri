# Unified controller input

Status: transport implementation. The user approved first-message authentication with the existing portal capability. Production routing is unchanged. Shared-seat assignment and configuration still need grounded decisions.

## Approved behavior

The user approved native evdev input, authenticated WebSocket delivery through local korrid, and a persistent pool of `n` virtual gamepads. The default is four. Physical and remote controllers share that pool. Each connected controller can navigate the active portal.

Inactive portal input does nothing. Access revocation is not required. Do not retain gameplay input for later portal delivery. Preserve the neutral-before-rearm guard and host-owned system shortcuts.

Do not use CDP for controller delivery or rebuild Chromium. Work and tests run off-device. The handheld stays untouched until separate deployment approval.

## Source grounding

Legacy reference: `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62`.
Main reference at start: `133474a41a64366f4bd4df1f684db0f1ad1019c4`.

| Existing implementation | Preserve or change |
| --- | --- |
| `legacy:product/platform/input/native/wire-schema.ts` defines native events, device metadata, and subscriptions. | Preserve its wire names and shapes where the current producer and consumer need them. Rust owns exported wire types on main. Do not introduce an unrelated event language. |
| `legacy:product/platform/input/native-adapter.ts` receives WebSocket input and drops inactive input. | Preserve those behaviors and useful tests. Remove direct inputd access and diagnostic HTTP requests during input handling. |
| `legacy:product/platform/input/native/gamepad-mapper.ts` translates evdev events and axis metadata. | Preserve grounded mappings and tests. Integrate with the current semantic gesture/release contract rather than copying legacy timer behavior blindly. |
| `clients/portal/src/input/gamepad-adapter.ts` retires input on blur, visibility loss, and disconnect. | Preserve its neutral-before-rearm and gesture-release guarantees in native mode. Do not emit duplicate native and browser actions. |
| `services/inputd/src/input_seat.rs` creates four devices at boot and retains them across launch leases. | Keep startup ownership and neutralization. Replace the fixed count only after its configuration source and bounds are resolved. |
| `services/inputd/src/input_seat_receiver.rs` separates device lifetime from launch-scoped mirror authority. | Preserve credential checks and exact-launch authority. Shared physical seats must not depend on a Sunshine launch. |
| `services/inputd/src/devices.rs` selects one normalized target and rejects multiple matches. | Replace single-source selection with validated multi-source handling. Keep provenance checks. Do not fall back to arbitrary raw devices. |
| `services/inputd/src/virtual_targets.rs` routes local input to separate portal/game devices. | Replace those destinations with the shared pool in one production cut. Do not retain a compatibility route. |
| `services/korrid/src/portal_access.rs` checks capability, origin, and RPC permission. | Reuse current browser authority. An authenticated WebSocket contract is not present in this module. |
| `clients/portal/src/runtime-config.ts` consumes the private `window.KorriRpc` bootstrap. | Keep both bootstrap calls before mount. Do not publish capability values in URLs, assets, storage, or logs. |

The new transport avoids Chromium gamepad discovery for portal navigation. It does not change controller support in unrelated browser games.

## Decision gates

Do not implement ungrounded schema to make the plan look complete.

1. Resolved on 2026-09-28: send the existing portal capability in the first WebSocket message. Check the exact portal origin before upgrade. Authenticate before sending metadata or events. Bound handshake time, message size, connection count, and queued data. Never log or persist the credential. Do not accept controller injection from the browser.
2. Resolve shared-seat assignment from the existing seat-state rules. Current remote sources take the first free seat. A disconnect neutralizes the seat and reserves it for that controller number until the launch ends. Determine how that rule applies to physical sources with no launch boundary before implementing it.
3. Resolve the producer, name, and allowed range of the seat-count configuration. The user chose default four, not a settings schema or maximum.

Changes to source identity, reservation lifetime, overflow, or remote authority require an explicit decision when current or legacy contracts do not cover the case.

The source audit found these specific gaps:

- Legacy `product/platform/input-seat/policy.ts` defines `playerCount` for a launch companion, limited to zero through four. That is not a daemon pool configuration contract.
- Validated normalized controllers currently share name, input ID, and absent physical/unique IDs. `DeviceDescriptor::stable_identity` cannot distinguish several such controllers for reconnect reservation. Ground source identity in actual InputPlumber provenance before extending it.
- Legacy exposes explicit seat leave. Main clears reservations on launch end. Neither defines when to release a disconnected physical controller's reservation in a launch-independent pool.
- Neither branch has a browser capability WebSocket handshake. The legacy server upgrades without bearer or origin checks. Main's authenticated Nostr relay socket uses a different principal and protocol.
- Legacy native input clearing does not enforce neutral-before-rearm. Its default stale timer expires at 250 ms, before its 400 ms repeat delay without refresh events. Preserve useful behavior, not these defects.

## Implementation order

### 1. Protect the existing guarantees

Expand the real `SeatRuntime` regression before changing ownership. Hold input on every seat, end the lease, and verify neutral state without device recreation. Reject old authority. Start a new lease on the same devices and verify fresh input. Stop the runtime and verify release.

Run the existing seat receiver and reconciliation tests off-device. These tests establish current behavior, not complete shared-seat support.

### 2. Port the native input path

Extract the necessary legacy wire types into Rust and regenerate TypeScript through `korrid-check`. Add local korrid delivery and a portal native adapter. Keep the surface treaty unchanged.

The authentication choice is approved. Preserve the existing `Bearer ` credential representation from `PortalAccess::authorize` in the first text frame rather than adding an authentication object schema. The following subscription and server events retain the legacy wire contract. Keep the legacy root WebSocket URL path when mounting it on local korrid; do not enable it on the LAN peer router.

Test the real transport with correct and incorrect authority and origin. Test disconnect, reconnect, disposal, malformed input, finite buffers, and slow consumers. Test inactivity and frozen-portal backlog explicitly. A live socket or ready inputd process is not proof of navigation.

### 3. Unify persistent seats

After the assignment and count decisions, make one pool own the virtual devices independently of physical connections or remote launches. Feed validated normalized physical input and authenticated remote input into that pool. Keep device identity stable for the pool's lifetime. Do not promise fixed kernel event-node numbers across restart.

Use native systemd and Nix configuration at their existing ownership points. Do not copy service configuration into a plugin declaration. Remove separate local portal/game targets in the same runtime cut that installs the shared path.

### 4. Verify the integrated path

| Area | Required evidence |
| --- | --- |
| Pool lifetime | Four default seats and a configured count above four remain present across disconnect and launch transitions. Empty seats remain neutral. |
| Mixed sources | Physical and remote sources use the same pool without collisions or unauthorized writes. All assigned controllers can navigate. |
| Activation | Inactive input emits no portal action. Return discards old input and waits for neutral before rearming. |
| Lifecycle | Source loss, transport loss, restart, reconnect, and device renumbering cannot leave held input or duplicate delivery. |
| Shortcuts | Host-owned shortcuts remain usable during gameplay and portal navigation. |
| Authority | Invalid capability, origin, or remote launch authority cannot receive or inject controller input. Secrets do not reach logs or URLs. |
| Production selection | Linux native mode does not also consume the browser Gamepad API. Browser development retains its explicit in-memory path. |

Use existing tasks discovered through `nix run .#help`: `inputd-check`, `korrid-check`, `portal-check`, and `rpminiv2-check`. Run focused checks during development and the affected integrated checks before landing. Generated contracts are read-only.

### 5. Land, without deploying

Review and commit in the worktree with the secret hook enabled. Rebase onto current `main`, fast-forward, and remove the completed worktree and branch. Leave unrelated main changes untouched.

Report off-device results separately from hardware acceptance. Request separate approval before any handheld operation. Do not remove the per-boot browser view or startup override during this work. Future delivery must follow `nix/device-cache/README.md`, without device builds or signature bypasses.

## Verification record

The expanded lifecycle regression passed off-device on 2026-09-28. `cargo fmt --check` and targeted Clippy with warnings denied also passed.

| Suite | Result |
| --- | --- |
| `input_seat` | 18 passed, including held-input lease transitions, arrival-order assignment, and disconnected remote reservations. |
| `input_seat_receiver` | 6 passed using the real receiver process and local sockets with its dry-run backend. |
| `runtime_reconciliation` | 23 passed. |

Run from this worktree with `KORRI_ROOT` set to its absolute root:

```sh
nix develop .#inputd --command cargo fmt --manifest-path services/inputd/Cargo.toml --check
nix develop .#inputd --command cargo clippy --manifest-path services/inputd/Cargo.toml --test input_seat -- -D warnings
nix develop .#inputd --command cargo test --manifest-path services/inputd/Cargo.toml --test input_seat --test input_seat_receiver --test runtime_reconciliation
```

The latest inputd run passed 47 tests, plus formatting and targeted Clippy. These tests do not verify shared physical/remote routing.

The browser transport in `clients/portal/src/input/native-connection.ts` sends the existing bearer frame followed by the unchanged legacy gamepad subscription. It uses only the private local korrid port, clears held input through its disconnect callback, bounds received messages, and preserves legacy reconnect timing. Authentication failures do not reset backoff on upgrade. The caller must still provide active-state and mapper lifecycle handling.

`nix run .#portal-check` passed 272 tests and TypeScript checking. Eleven tests exercise the new connection against a real local Bun WebSocket server. They cover frame order, credential-free URLs, reconnect, disposal, malformed/binary/oversized data, and repeated authentication rejection. The first run passed tests but found widened fixture string types; typed treaty fixtures fixed that failure.

The latest Rust transport run passed 21 tests using real loopback WebSockets and the original legacy wire fixtures. All eight existing portal-authority tests also passed. The transport source validates serialized size, finite numbers, device lifecycle, and metadata count. It captures metadata and attaches live queues under one lock. Dropping the producer terminates the router's source lifetime and closes its sockets.

The dependency audit found that Tungstenite logs complete messages at trace level and close-frame contents at debug level. A real-socket regression with an installed trace logger reproduced a credential leak. The test records only a leak verdict, never the credential or raw log. `log` compile-time `max_level_info` and `release_max_level_info` features now block those diagnostics even if an embedding process installs a trace logger. Cost: all debug/trace diagnostics through that log facade are unavailable in this build. The regression failed before the feature change and passed afterward with the trace logger still enabled. Formatting, treaty regeneration, and the affected Rust and portal tests passed again after review fixes.

Strict korrid Clippy found 11 existing warnings in unchanged code. The new transport passed scoped compilation and formatting. No unrelated warnings were suppressed or repaired. Release-mode and full korrid checks have not run.

Independent review found no blocker in this intentionally unwired slice. Both suggested improvements are applied. The wire test now compares serialized output against the original legacy fixture, including optional fields. The browser rejects oversized strings before allocating a UTF-8 copy, and valid oversized JSON tests cover both character and byte limits.

The transport is not mounted in production. The remaining freshness problem includes bytes already accepted by TCP or buffered in a frozen browser. Server queue limits alone do not solve it. Initial held-state synchronization and the native semantic mapper also remain unimplemented.

No device operation ran. The work is not ready to land.
