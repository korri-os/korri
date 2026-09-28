# Unified controller input

Status: checkpoints `8c2114c2`, `e9aef9ac`, and `c53d9973` are committed. Transport, pool logic, count storage, and truthful settings editability pass their scoped checks. Controller production routing is unchanged. The user approved the InputPlumber read-only target-node mapping next; physical reconnect identity and complete native producer integration still need grounding.

## Approved behavior

The user approved native evdev input, authenticated WebSocket delivery through local korrid, and a persistent pool of `n` virtual gamepads. The default is four. Physical and remote controllers share that pool. Each connected controller can navigate the active portal.

Inactive portal input does nothing. Access revocation is not required. Do not retain gameplay input for later portal delivery. Preserve the neutral-before-rearm guard and host-owned system shortcuts.

A disconnected controller keeps its seat reserved until the current session ends, including while paused. The same controller reclaims that seat on reconnect. When the session ends, free disconnected reservations. With no active session, free the disconnected source's slot immediately. The virtual gamepad stays present and neutral. Reservations can block a replacement controller while all slots remain occupied or reserved.

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
2. Resolved on 2026-09-28 through ask `0807f30b-438a-4b38-91a5-2d7f01534916`: the user selected `reserve-through-session`. Preserve a disconnected controller's reservation during the session, including pause. Release disconnected reservations when the session ends or immediately when no session exists. Physical reconnect identity still needs real producer evidence.
3. Configuration ownership is resolved: korrid manages the device's seat count. Changing it must not require a Nix deployment. Nix installs the service and permissions. Pool recreation can occur only with no active session. The Nix-option recommendation was withdrawn after the user questioned deploy-to-change configuration. The user agreed to korrid ownership. Resolved through ask `bfbcdb38-687c-44f3-af1a-1aef03c42fb7`: the user approved `host.preferences.playerCount` in the existing `device.yaml`. It is device-only, not inherited or overridden by games. Default four; valid integer values are 1–255. This is a representation limit, not a hardware-support claim. Use the existing conflict-safe writer, not another file. Refuse count changes during active sessions, including pause.

4. The user approved `SettingsSnapshot.editableSettingIds` after ask `8ead77c6-5dfa-45e4-bc08-573e68b91449`. It is a mandatory list of existing setting IDs with supported, authorized write handlers. It grants no authority. Runtime preconditions remain checked on write. Keep uneditable values visible and omit their existing `interaction` declarations. No persistent setting or new surface-treaty field is needed.

5. The user approved the recommended option from ask `00fd4155-0004-4c3c-8647-4b33b46b2e02`, after its plain-language explanation. Add read-only `org.shadowblip.Input.Target.DevicePaths` to InputPlumber, reporting each target's actual evdev nodes. Inputd follows the existing composite-to-target mapping and validates the reported nodes before opening. Preserve existing controller identity fingerprints. Build InputPlumber off-device, not Chromium. Add only the narrow metadata-read permission needed for this lookup. No handheld deployment is authorized.

Changes to source identity, reservation lifetime, overflow, or remote authority require an explicit decision when current or legacy contracts do not cover the case.

The source audit found these specific gaps:

- Legacy `product/platform/input-seat/policy.ts` defines `playerCount` for a launch companion, limited to zero through four. That is not a daemon pool configuration contract. Main's existing `config/settings.rs` edits the fixed `device.yaml` through conflict-safe writes. However, the `SettingsSnapshot` and `SettingsUpdate` handlers in `lib.rs` reject Linux host mode. Do not assume that adding a field alone produces a usable Linux setting.
- Validated normalized controllers currently share name, input ID, and absent physical/unique IDs. `DeviceDescriptor::stable_identity` cannot distinguish several such controllers for reconnect reservation. Ground source identity in actual InputPlumber provenance before extending it.
- The pinned InputPlumber 0.75.2 source confirms a specific missing link. `src/dbus/interface/composite_device.rs` exposes `SourceDevicePaths` and `TargetDevices`, but target paths are DBus objects. `src/dbus/interface/target/mod.rs` exposes target Name and DeviceType, and the gamepad interface exposes Name. Neither provides its evdev node. `src/input/target/xpad.rs` provides no distinct phys/uniq identity. Do not associate several identical normalized pads with physical sources by enumeration order or volatile `eventN`. A native producer change needs a deliberate contract and permission review.
- Legacy exposes explicit seat leave. Main clears reservations on launch end. The user extended session-scoped reservation to physical sources. No permanent reservation or timed eviction is required.
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

The internal `services/inputd/src/seat_pool.rs` engine now owns persistent devices and implements the approved reservation lifecycle. It accepts an explicit count and opaque source keys from a future coordinator. It does not decide persisted configuration, physical identity, remote authentication, or source mapping.

Production integration must feed validated normalized physical input and authenticated remote input into that pool. Keep device identity stable for the pool's lifetime. Do not promise fixed kernel event-node numbers across restart.

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

## Pool checkpoint

The pool engine implements first-free allocation, shared caller-supplied key space, same-source reservation reclaim, no stealing, exact session end, and connected-assignment retention. Pause has no pool transition. Devices survive disconnects and session changes. The engine cannot resize itself. Its caller supplies the count, with four as the existing default.

An off-device failure-injection test reproduced a uinput writer-cache fault in the production encoder. After a partially applied write failed, a neutral retry emitted zero events because the old cache still said neutral. The fix marks cached state unknown before emission. A retry then emits all 11 buttons and eight axes. Successful writes still use deltas and deduplicate repeated state.

The real backend now accepts positive existing u8 slots beyond four and recognizes canonical seat names through that representation. This does not prove installed permissions. `plugins/sunshine/99-z-korri-sunshine-input.rules` still grants seat access only for P1–P4. Moving pool ownership out of Sunshine and extending permission coverage remain production-integration work. No permission rule changed here.

| Latest inputd check | Result |
| --- | --- |
| Library tests, including actual encoder recovery | 40 passed. |
| Existing remote seat runtime | 18 passed. |
| Existing receiver process and local sockets | 6 passed. |
| Runtime reconciliation | 23 passed. |
| New pool engine | 18 passed, including an explicit six-seat pool with a recording backend. |
| All-target Clippy with warnings denied | Passed. |
| Formatting and whitespace | Passed. |

These 105 tests ran off-device. Independent source review found no blocker in the intentionally unwired pool and writer changes. A stale test comment about the old four-slot backend restriction was corrected. Kernel-level six-seat creation and physical reconnect identity are not verified.

## Count storage and RPC checkpoint

`host.preferences.playerCount` now decodes as a device-only integer from 1 through 255. The resolved default is four. The existing fixed-file writer changes that leaf under its revision checks and preserves unrelated configuration values and catalog bytes. Host-specific preferences keep the count out of games, profiles, runners, families, and releases. No Nix setting or extra configuration file was added.

The existing Settings RPC uses the approved field path as its setting ID. Linux Host mode reads settings and accepts only count writes. LocalSessions authorization permits only that exact new setting ID; unrelated settings remain blocked. ReadOnly cannot write. Peer, plain LAN, private-control, and Brain paths cannot bypass the new local idle gate.

The idle gate holds the same transition mutex as prepare, freeze, and stop across the complete configuration write. It also requires empty live-unit enumeration and no active journal record. It rejects active, paused, stopping, focus-failed, or uncertain states without changing game/freezer state. This stores the count; it does not yet apply it to the live pool.

A regression test found that an empty host preference map returned the default count but remained classified as unsupported. The fix makes an absent count usable even after a manual leaf deletion. Unimplemented launch preferences remain rejected. The test failed before the fix and passed afterward.

The parent integrated check passed 168 affected Rust tests and all 291 portal tests, plus TypeScript checking, formatting, treaty regeneration, and whitespace checks. The 19 new browser tests use the existing HTTP request shape and in-memory implementation. The real-file/RPC tests cover bounds, CAS conflicts, preservation, failed writes, active-session refusal, authority, and prepare/write serialization. The full korrid gate and release build remain unrun.

**Resolved review blocker:** `SettingsSnapshot.editableSettingIds` now intersects supported write handlers with the actual caller's authorization. Host Full/LocalSessions advertises only the count. Host ReadOnly advertises none. Brain Full advertises name and credential writes, but not count or installed-plugin toggles. Both successful reads and writes carry the metadata. Forging it cannot grant permission. The host adapter omits unsupported interactions while preserving values. Count presentation is not part of this fix.

This is a binding change, not new view structure. Keep existing rows and container behavior. The [Klaviyo settings reference](https://mobbin.com/screens/faa8603d-1547-4ae3-b076-e6074917288c) shows email and role as plain information alongside explicit name inputs. Retain that distinction through Korri's existing read-only row variant. Do not copy the reference's navigation or introduce disabled-looking inputs for unsupported operations.

The integrated editability check passed 172 affected Rust tests, all 301 portal tests, and all 572 Pico tests. TypeScript checking, Rust formatting, treaty regeneration, Pico authoring/coverage gates, and whitespace checks passed. Independent review found no remaining blocker in this checkpoint. Strict korrid Clippy still has the previously recorded unrelated warnings.

The preview client now accepts a real settings snapshot as fixture data. Its private authorization set cannot be expanded by editing the seed or returned metadata. Settings responses are cloned, and real HTTP permission refusals produce PermissionDenied rather than BrainUnreachable.

A controller-accessibility test also failed before the Pico fix: read-only settings had tabIndex -1. Pico now opts only fact settings into native group focus, with unique label/value references and no action handler. Shift already uses this pattern. The actual Shift integration and Pico tests prove readonly values remain focusable without opening editors or invoking writers. No CSS or layout algorithm changed. Live browser geometry and Caliper HMR were not exercised.

## Target-node mapping checkpoint

The patched InputPlumber release build and package check passed off-device. The release library and binary suites each passed 23 tests with one kernel-device test ignored; four doctests passed. The successful outputs are `/nix/store/i73lx1y0yfj5630hi0m9q39j181b6bh5-inputplumber-korri-0.75.2` and `/nix/store/ivsjg35a7af21jjx22cvsd2m786cgfh4-inputplumber-korri-package-check`. Parent-managed process `proc_6af5` verified both outputs. Earlier attempts failed because the test bus inherited host session configuration; those results are not the accepted build.

Inputd's initial discovery implementation passed 124 scoped tests and module evaluation. Integration inspection found a concrete mismatch: the installed `60-xbox_one_gamepad.yaml` creates `xb360`, `mouse`, and `keyboard`, while only Xbox targets currently implement DevicePaths. The corrected consumer selects normalized targets through existing DeviceType metadata, ignores empty/volume-only persisted composites, and compares metadata sets rather than order. Its 21 private-DBus cases passed within an 88-test rerun, with all-target Clippy and formatting passing.

The isolated NixOS VM proof passed in 52 seconds through parent-managed process `proc_4514`. It created two identical real Xbox targets only inside the guest. Inputd read their distinct kernel nodes through the real system bus and polkit. Unrelated users were denied direct reads, and inputd was denied GetAll, Set, and CreateTargetDevice. Existing Stop/LoadProfilePath grants remained intact. No host input device was passed through.

**Provider security correction verified:** review found a path outside the first direct-access checks. zbus ObjectManager calls getters without a caller header for GetManagedObjects and InterfacesAdded. Upstream check_polkit accepts a missing header, so the initial getter exposed DevicePaths without authorization. The corrected getter rejects a missing header locally, before the unchanged shared helper. Three new regression cases failed on the old getter and passed after the fix.

Parent-managed process `proc_ad4f` rebuilt the corrected provider, module policy check, and expanded VM check successfully in 417 seconds. Both release suites passed 25 tests with one kernel-only test ignored; four doctests passed. The real-kernel VM proved authorized direct reads still work and successful ObjectManager replies plus broadcasts omit DevicePaths. The corrected provider is `/nix/store/2kzj0j0n1pm4hzfrmpdjgqhxn5x6yb34-inputplumber-korri-0.75.2`; the VM result is `/nix/store/dnxcvzrqhchn6933fv3jmnrhj78fn3md-vm-test-run-korri-inputplumber-device-paths`; the module result is `/nix/store/bx6j17q7y7ffpiag3z8f32vzi1n7rm7d-korri-input-module-check`. Patch SHA-256 is `a62c92e8f4bad5ea393b7b772597ce7684d412a4a1f9149e37bc6297396b3013`. The earlier i73lx1y0 output is superseded and must not be deployed. All long builds now belong to the parent's visible process tool.

The complete `korrid-test` task also passed after running the existing locked script-fixture setup. Its log is `/tmp/pi-processes-TnenuC/proc_4abf-stdout.log`. This is not the remaining full `korrid-check` release/smoke gate.

No device operation ran. Controller capture, native delivery, pool application, and deployment remain incomplete. The work is not ready to land.
