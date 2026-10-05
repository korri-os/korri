# Local launch transition handoff

Research against commit `516a1da77`. This document records findings, not an approved implementation or contract design.

## Conclusion

The library flash needs a shared live-session correction, not a delay or a Pico-only change. The existing launch screen is sufficient. The missing connection is between that screen and the exact live session taking ownership of the game window.

korrid already observes that ownership. Its current replies do not give the portal that fact. Keeping the current Busy state alone is also insufficient: the portal does not process normal session observations in that state.

## Findings from primary sources

| Finding | Source and evidence |
| --- | --- |
| The launch screen already exists and must replace the library during startup. | [PicoLaunchStage](../../surfaces/pico/src/ui/organisms/PicoLaunchStage.tsx), [screen selection](../../surfaces/pico/src/pico-screen-view.ts), and design commit `02ffe8318`. Status outranks the catalog. |
| The runner-selected path never activates the shared launch screen. | [SurfaceRoot](../../clients/portal/src/surface/SurfaceRoot.tsx), [runner chooser](../../clients/portal/src/surface/runner-chooser.ts), and `beginCatalogLaunch` in [use-launchables](../../clients/portal/src/surface/use-launchables.ts). The chooser has its own Busy state. Global status remains Browsing. Success closes the chooser. |
| Ordinary command launches also return to browsing before window ownership is known. | `withLocalCatalogPrepareOutcome` in [launchables state](../../clients/portal/src/launchables/state.ts) converts successful prepare to Ready. [surface-model](../../clients/portal/src/surface/surface-model.ts) maps that to Browsing. |
| Launch acknowledgement and phase `running` do not prove window ownership. | `prepare_inner`, `status`, and `reconcile_portal` in [session state](../../services/korrid/src/host/session_state.rs). A live unit can report Running without a mapped game window. [RPC mapping](../../services/korrid/src/lib.rs) exports that as `running`. |
| korrid already has the correct ownership authority. | `focused_ownership` in [compositor focus](../../services/korrid/src/host/compositor_focus.rs). It requires one eligible window owned by the exact launch and one focused node matching that window. It does not trust a title, another window, or a focus-command acknowledgement. |
| The current session treaty omits this ownership observation. | [Rust RPC mapping](../../services/korrid/src/lib.rs) and [generated treaty](../../contracts/generated/korrid.ts). SessionPrepared carries game and launch identity. SessionStatus carries active and overlay. Neither communicates initial window handoff. Leave-overlay authorization is not readiness. |
| Retaining Preparing or Launching alone blocks recovery. | The local-session poll in [use-launchables](../../clients/portal/src/surface/use-launchables.ts) only reads status while Ready. `withSessionStatus` in [state](../../clients/portal/src/launchables/state.ts) also accepts only Ready. Recovery loads preserve non-Ready command locks. |
| Browser freezing cannot substitute for a reliable observation. | `reconcile_portal` in [session state](../../services/korrid/src/host/session_state.rs) treats the optional freezer as best effort. The [backend watcher](../../services/korrid/src/host/mod.rs) observes focus and completion independently of browser activity. A frozen browser cannot be required to receive a final reply. |

These findings come from direct source inspection. Focus proves screen ownership, not the first rendered game frame.

## Executed probes

The parent executed these workstation-only probes. No target operation occurred.

| Probe | Result |
| --- | --- |
| Existing portal/state/Pico tests across eight files, via `/tmp/korri-launch-research-tests.sh`. | 143 passed, zero failed. The suite misses the full selected-launch presentation connection. |
| Real SurfaceRoot and PicoSurface with the shipped in-memory korrid client, via `/tmp/korri-launch-state-audit.sh`. | Ordinary command startup reached the launch screen. Runner-selected startup remained Browsing and failed the launch-screen assertion. Both paths returned Browsing after acknowledgement. |
| Same audit, retaining Preparing after acknowledgement through the real reducer. | An authoritative SessionCompleted observation left the state Preparing. This disproves the simple keep-Busy shortcut. It is not a separate defect in the current guard for an unacknowledged command. |

The audit produced one passing test and two deliberately failing assertions. The probes are temporary research artifacts, not committed regression tests. Native compositor tests were inspected, not executed for this research.

## Systemic direction and cost

Recommendation: connect both existing local launch paths to one portal-owned transition. Ground its handoff in korrid's existing exact-launch window ownership. Keep the existing surface presentation and revise session observation, ordering, and recovery together.

The correction must preserve these distinctions from [CONTEXT.md](../../CONTEXT.md):

- Leave freezes the live session and gives input to Korri. It is not incomplete initial startup.
- Return addresses the exact live session. A missing window or refused focus can leave that session alive.
- End and natural completion resolve the exact session. A pending stop is not completion.
- Cancel currently closes the chooser, not an accepted launch. Late success must retain its exact identity.
- An observation error is not proof that the live session ended. Old replies must not replace newer sessions.

**Cost:** this crosses korrid, the session treaty, and portal lifecycle handling. Merely closing the chooser later does not correct the observation gates. A fixed delay does not correct slow startup, focus loss, or a launch that never opens a window. Window ownership alone also cannot correct a black game window that has not drawn its first frame.

## Unresolved before implementation

The representation of the existing ownership fact remains unresolved. This research invents no fields, variants, persisted records, or migration.

The recovery and cancellation policy for a live launch that never takes the screen also needs an explicit decision. Do not silently End such a session or invent a timeout.

Validation must cover both paths, delayed/missing/ambiguous windows, early exit, launch errors, stale replies, unavailable status, warnings, Leave/Return/End, focus loss, reload races, and browser freeze/thaw. Native compositor and device verification are still required before claiming a complete fix.
