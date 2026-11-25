# Idle power-off (#614)

Owner decisions: issue #614 and both 2026-11-24 comments. Only dedicated
Linux play devices enable this. A running game always keeps the device on.
Power-off is the current action; verified suspend remains a later device cut.

## Existing signals

| Signal | Source |
|---|---|
| Battery discharge | `services/korrid/src/battery.rs`: `power()`, `Power.discharging`; unknown power prevents shutdown. An online external supply also inhibits, even if a weak charger leaves the battery discharging. |
| Controller, key and touch input | `services/korrid/src/surface_host_input.rs`: validated `publish` / `publish_frame`; `clients/shell/src/shell.rs`: `activity()` also sees compositor pointer input. |
| Live game, including frozen or stopping | `services/korrid/src/host/mod.rs`: `session_status()`; `host/session_state.rs`: `HostSessionStatus`. Recovery failures prevent shutdown. |
| Hosted stream | The host's live session above; `host/stream_players.rs` owns attached viewers. |
| Viewed stream | `services/korrid/src/upstreams.rs`: `UpstreamRegistry.selected_launch_id()` (also used by `acquisition/play.rs`). |
| Downloads and installs | `services/korrid/src/acquisition/mod.rs`: shared install queue; `acquisition/game.rs`: provider jobs; `acquisition/pieces.rs`: device install jobs. Queued and paused work also keeps the device on. |
| Save sync | `services/korrid/src/saves/peer/{mod,sync,object_route}.rs`: incoming/outgoing sync and payload streams. Add scoped process-local work accounting at these existing effects, not a persisted sync state. |

## Composition and warning

Follow the product's hardware facts (`sleep.states`, `battery.verified`) with
one boolean, `services.korriProduct.dedicatedToPlay`, default false. Device
modules set it, not runtime preferences. Handhelds and Tab S6 set true; the
Android app receives neither this Nix configuration nor an enabled worker.
No new persisted data or plugin declaration is needed.

One korrid policy owns monotonic inactivity time. Battery events remain the
existing reader's responsibility (ADRs 0092 and 0094). A bounded once-per-second
policy check reads existing in-memory work and session facts. Inhibitors reset
the timer, so unplugging or ending work starts a fresh 15 minutes. An unknown
or failed signal inhibits rather than risking a game or transfer.

At 14 minutes, publish the existing `SurfaceStatus::Confirm` with one enabled
"Keep on" action and the same decline action for Back. Existing surface
confirmation components render it; the shell draws no UI. Cancel or any input
starts a fresh countdown. Reserve at least 60 seconds after a warning is
actually delivered before acting. No warning delivery means no power-off.
Use the existing surface watch acknowledgement to prove delivery.

The only new treaty request reports local input activity from the shell. It
contains no timestamp or hardware details. Throttle continuous pointer input
at the shell edge, but send the first event immediately. Raw controller input
is also counted before shell delivery. This keeps compositor touch and key
input covered without giving a surface hardware knowledge.

The OS effect is a separate logind power-off adapter, authorized only for the
korrid service on dedicated devices. No shell, package or plugin performs it.
Replacing that adapter with device-proven suspend does not change the policy
or warning flow. Manual power-off and existing display blanking are unchanged.

## Proof and remaining hardware work

Tests first: deterministic policy time, every inhibitor, cancellation, warning
delivery, signal errors, and no premature/repeated effect. Exercise the real
surface model/request/watch paths and the OS adapter against a private D-Bus
service, never the build machine's logind. Check the Tab S6 and RP Mini V2 Nix
modules. Device acceptance needs parent-coordinated time; do not deploy to
Tab S6. Verify battery unplug/plug, input, games, streams, downloads, save sync,
warning cancellation, and unattended clean shutdown on an approved device.

The implementation also covers Steam engine installs in `steam_engine/store.rs`
and the engine's live-host counter in `steam_engine/mod.rs` (which stays held
through exit Cloud sync, `steam_engine/sessions.rs`). Pending launch reservations
also inhibit. The product's exact polkit policy remains identical on every
device; only dedicated devices give korrid membership of `korri-idle-power`.
That group grants power-off, not ignore-inhibitors or suspend.

The warning wakes the existing watched displays through `HostRuntime.wake_displays()`; ordinary display blanking must not hide the warning. A failed wake cancels it.
