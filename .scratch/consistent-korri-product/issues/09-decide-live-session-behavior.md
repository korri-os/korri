# Decide what happens when leaving and returning to a game

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: resolved
Blocked by: 01, 02

## Question

What must happen to an active game and its input when the user returns to Korri, returns to the game, invokes a plugin action, or ends the session?

The required controls are settled by [Define the intended Korri product](01-define-intended-product.md#live-game-controls). This ticket specifies their observable behavior, not whether they belong. In particular, the baseline does not decide whether opening Korri automatically pauses a game. Distinguish keeping a process running, pausing it, and restoring saved state.

Inspect the actual [portal session effects](../../../clients/portal/src/surface/use-launchables.ts), [overlay composition](../../../clients/portal/src/main.tsx), [host session control](../../../services/korrid/src/host/control.rs), and supported plugin actions. The current overlay's in-memory controller is not evidence of live controls. Follow actual producers for input ownership and local versus streamed sessions; do not invent identical pause or focus behavior for every runner.

Resolve with Simon the local and streamed cases, unsupported actions, and visible failure behavior. Preserve the existing requirement that Korri owns sessions and cleanup. Ending a session must not silently act on a different launch. Define what evidence proves the behavior before the shared product-boundary ticket chooses integration ownership and acceptance checks.

Automatic restoration after shutdown and cross-device save synchronization are deferred by the product decision. Do not add either as a prerequisite. Do not implement a new overlay, protocol, or session engine in this decision ticket.

## Comments

2026-09-20. Grilled with Simon in three rounds through the ask tool. Facts checked in source at `be325b6b` before each round: `services/korrid/src/host/session_state.rs` (`HostSessionStatus` is `Running | Frozen | Stopping | Completed | NoActive | RecoveryBlocked`; `freeze`, `thaw`, and `stop` each take an exact launch id and answer `StaleIdentity` for any other launch; input-seat leases stay alive while frozen), `services/korrid/src/host/compositor_focus.rs` (thaw raises the exact launch's Sway window; `FocusFailed` means the game runs but the raise failed), `services/korrid/src/host/input_seat.rs` (the seat lease protocol has START and STOP only, no reset), `services/korrid/src/lib.rs:1619-1745` (prepare, freeze, and thaw are forwarded to the far peer for a streamed launch; the local-control listener refuses freeze and thaw in brain mode), `clients/portal/src/surface/use-launchables.ts:470-545` (the portal calls thaw for resume and stop for end; nothing calls freeze), `clients/portal/src/main.tsx:25-60` (the gameplay overlay runs against an in-memory fixture because korrid performs no in-game control on Linux yet), `services/korrid/SCRIPTING.md` "Effects are a closed first-party vocabulary" (session controls exist for RetroArch and Moonlight only; a DIY runner such as `plugins/ppsspp/plugin.ts` declares none), `services/korrid/src/lib.rs:355-361` (control failures are `StaleSession | UnknownControl | Disabled | InvalidValue | Unavailable`), `services/inputd/src/action_catalog.rs:178-220` (Home tap is `SystemPanel`, a chord is `KillCurrentGame` with exact stop, and `PowerSuspend` has no trigger on any device), `services/inputd/src/korrid_client.rs:66` (the kill chord reads status and stops only that launch). No device module wires the system panel command. No stream viewer exists for Linux. korrid has no sleep hook.

Simon added one boundary in round 2: leaving a streamed game is not one verb. Ending kills the far game; putting the device to sleep freezes it. Round 3 confirmed that as the two-verb model and pulled device sleep into scope.

## Answer

Resolved 2026-09-20. All choices are Simon's, selected through the ask tool.

### Terms

- **Live session**: the one exact launch korrid holds as active on a device, named by its launch id. Every control in this answer names that id and is refused for any other launch.
- **Leave**: the user moves away from the live session without ending it. Home tap and device sleep are the two ways to leave.
- **Return**: the user comes back to the live session. korrid thaws it and raises its window.
- **End**: the user destroys the live session. Portal end and the kill chord are the two ways to end.
- **Gameplay overlay**: the screen Home opens over a live session. It shows the core controls and the plugin actions korrid lists for that exact launch.

### Leave by Home tap

Home opens the gameplay overlay and korrid freezes the exact launch. The game uses no CPU, draws nothing new, and hears no input. Korri owns all input while the overlay or the portal is in front. A second Home tap returns to the game. The full portal is reached from the overlay, not from Home.

The same promise holds for a streamed session: korrid forwards the freeze to the far device, the far game freezes, and the near device shows the overlay.

### Leave by device sleep

Device sleep freezes the exact launch before the device suspends, local or streamed. On wake, korrid thaws only if the user was in the game when the device slept; a user who slept from the overlay wakes to the overlay. A device with no working suspend cannot keep this promise; the hardware policy from [Define hardware limits and product acceptance](02-define-device-support.md#answer) must record that as an explicit limit, not omit the behavior.

### Return

Return thaws the exact launch and raises its window. Input that arrived while Korri was in front is discarded: the game sees a neutral controller state, then live input. If thaw succeeds but the window cannot be raised (`FocusFailed`), the overlay shows the failure, the game stays running, and return remains available. Korri does not refreeze and does not end the session for a display fault.

### End

Portal end asks once, then stops the exact launch. The kill chord acts at once with no confirmation; it is the escape hatch for a game that eats input. For a streamed session both stop the exact far launch and the local viewer. If the near device cannot confirm the far stop, it says so; it does not report the session ended.

There is no third verb. "Close the viewer and leave the far game running" is not a product behavior.

### Plugin actions

The overlay shows the core controls (return, end, open Korri) plus only the actions korrid lists for the exact session. A runner with no session controls, which today is every runner except RetroArch and Moonlight, shows core controls only. Nothing greyed, nothing fixed.

A refused action (`StaleSession`, `UnknownControl`, `Disabled`, `InvalidValue`, `Unavailable`) or an unanswered runner shows a short notice on the overlay naming the reason. The session is untouched and the overlay stays open.

### Exactness

Every freeze, thaw, stop, and action names the exact launch id. When korrid answers `StaleIdentity`, the overlay says that game is no longer running, reloads the session, and acts only on what the user chooses next. No control retargets to whatever runs now.

### Evidence

Simon selected automated evidence: korrid state-machine tests for freeze, thaw, stop, `StaleIdentity`, `FocusFailed`, and recovery after a korrid restart. Physical per-device passes, a near/far streaming pass, and a sleep/wake pass were offered and not selected. [Decide the shared product and hardware boundary](07-decide-product-device-boundary.md) owns where those tests run and whether any physical check is added for support status.

### Costs and limits

- Freezing cuts audio at once and times out online games while the user is in a menu. Simon accepted this.
- A far game frozen by a near device that never returns stays frozen until someone ends it or the far device's own recovery runs.
- Input discard on thaw needs a seat reset that the seat protocol does not have; the sleep hook, the overlay controller, the system panel wiring, and the Linux stream viewer are all unbuilt. This answer records behavior, not current capability.
- Automated tests prove korrid's state machine, not the compositor, the input seat, or a real device. Support status still follows ticket 02's acceptance rule.
- RetroArch network commands are one-way; "the runner did not answer" and "the command was ignored" show the same notice.

This answer authorizes no new schema, plugin declaration, protocol, or deployment. Automatic restoration after shutdown and cross-device save synchronization stay deferred; sleep is not shutdown.
