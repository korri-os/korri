# Launch handoff

## Decisions

The owner approved the direction in [Local launch transition handoff](../research/launch-transition-handoff.md): "This is the correct direction. Proceed if ready."

| Ask | Decision |
| --- | --- |
| Real Cancel (2026-10-05) | Cancel stops the launch. Before commit, korrid never starts the game. After acceptance, Cancel ends only that exact live session. |
| `e5690eca-ba68-4cba-a805-f0b2c82a0617` | korrid publishes startup facts, so a reloaded portal recovers the same launch without starting it again. |
| Owner reply, 2026-10-06: "Add a manual choice." | When a reloaded portal finds several pending launches, it offers one exact Cancel for each. Korri chooses, starts and cancels none of them by itself. |

## What a person sees

The launch screen stays up from the press until the window of that exact live session owns the screen. It does not go back to the library first. Cancel is on the launch screen in every surface. Back during startup is Cancel. Back during a choice between pending launches does nothing.

| Surface | Startup Cancel | Several pending launches |
| --- | --- | --- |
| Pico | A row under the launch stage. | One row per launch, with its state under it. The list scrolls when it is longer than the stage. |
| Shift | The rail holds only Busy actions. | One tile per launch. The hero names the focused launch in full, with its state. |
| Boxbuster | The deck under the TV, signed with its action. | Korri calls you to the TV. One signed deck per launch. The TV lists each launch and its state. Without a store, the door holds the same actions. |

## Treaty

These are the wire changes. Rust owns them, and `contracts/generated/korrid.ts` is the generated copy.

| Change | Meaning |
| --- | --- |
| `app.session.reserve {gameId}` returns `SessionPrepared {gameId, launchId}`. | korrid names the launch before any effect. The reservation is bound to the portal capability and person that made it. |
| `app.session.start {gameId, expectedLaunchId, runnerId?, overrides?}` | Starts exactly that reservation. |
| `app.session.cancel {expectedLaunchId}` returns `SessionStopOutcome`. | Cancels exactly that launch. `pending` means cleanup continues. |
| `SessionStatus.pendingLaunches?: PendingLaunch[]` | Every pending launch the caller owns, with phase `reserved`, `preparing`, `committing` or `cancelling`. Never a selection. |
| `SessionStatus.observationFailure?: RpcFailure` | Native state is unknown. This is not idle, exit, cancellation or readiness. |
| `ActiveSession.focusOwnership?` | The current exact compositor observation: `launch`, `excluded` or `other`. |
| `ActiveSession.initialHandoff?` | `waiting`: this korrid started the live session and has not seen it own the screen. `observed`: it has. `recovered`: the live session predates this korrid process. |

The surface treaty adds `Busy.actions`. A surface renders the host's actions and never infers Cancel from a label.

## Rules the code keeps

- Acknowledgement is not readiness. A start reply alone never ends the launch screen.
- Exact compositor ownership is not the first rendered frame. A black window can own the screen.
- A late reply never stops, overwrites or brings back a replacement live session.
- An unknown observation never retires a launch.
- The persisted journal and permissions are unchanged.

## Limits

- Pending launches live in the korrid process. A korrid restart loses launches that had no unit yet. Live sessions recover from the journal and report `recovered`.
- Reservations do not expire. Only Cancel or start retires one.
- Metadata ordering is fixed for local `HostRuntime` only. The analogous Brain and upstream post-await races remain.
- Pico keeps its 2 px pixel floor. At 160×120 a long launch name is taller than its list, so the row is reachable but clipped.
- No device, controller, compositor or freezer test ran. Hardware acceptance is still required.
