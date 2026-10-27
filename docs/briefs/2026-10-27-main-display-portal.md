# Main-display portal — owner decision and measured evidence

## Current behavior, 2026-10-27

The owner rejected spanning a single portal window across physical displays.
This is a product decision, not a timeout adjustment. A device may have stacked
displays and another, differently sized display beside them; the arrangement
need not form one rectangular viewport. Future per-display surfaces require a
separate design. ADR0039 is amended accordingly.

The catalog portal shows on the main display only. `app.surface.displays`
returns one main-display rectangle for catalog requests, whether the surface
asks for main or all. Overlay requests keep the existing free-display facts and
placement. Game panes, dimming, exact PID/unit selection and Home/Return keep
their existing ownership and effects.

Pico's two-region presentation is kept in its source, but is unused on devices
for now. A single returned display selects its existing one-display layout.
No surface, portal contract, plugin or persisted schema is changed here.
CONTEXT.md terms retain their meanings and were not changed.

## Cold-start investigation on 754b6293a plus Pico

Each accepted run restarted the kiosk, applied its runtime recipe after the
stock readiness restore completed, waited120s, and captured the result. The
baseline is the earlier six-run sample. All30 later accepted samples below
had zero input-invalid and zero unknown observations. Their120s windows and
capture intervals were audited for `inputd_policy_match`; contaminated attempts
would be excluded and repeated.

| Runtime condition | ok | stalled | valid | End UTC |
|---|---:|---:|---:|---|
| Spanning baseline | 3 | 3 | 6 | 05:08:09 |
| Floating640×960 after stock restores | 3 | 3 | 6 | 13:40:21 |
| Main-only640×480 fullscreen; DSI-1 off | 6 | 0 | 6 | 14:12:27 |
| Both-axis extra620×900→global restore | 2 | 4 | 6 | 14:25:12 |
| Extra640×900→global restore at+1.5s | 2 | 4 | 6 | 14:38:00 |
| Extra640×900→global restore at+3s | 3 | 3 | 6 | 14:50:58 |

The late interventions were **additional third restores**. The unchanged
binary still performed the original synchronous restore and its500ms follow-up.
Actual extra-reflow targets were1.526–1.551s and3.034–3.050s after the first
catalog restore's journal monotonic timestamp. They do not test replacing the
compiled500ms constant with another delay.

No spanning recipe reached6/6. The main-only control did, with the portal at
(0,0,640,480), fullscreen_mode1, DSI-2 active/DPMS=true in all12 bounding output
reads and DSI-1 DPMS=false. This control still ran after the old binary's stock
spanning restores. It chose the product direction; it does **not** prove that
the clean candidate works with both old workarounds removed.

### Detector validation and positive controls

Two-display runs used the owner's header strip x16–256,y495–515. More than20
pixels with RGB sum>90 at+120s meant Loading/Ready remained visible rather than
attract. Zero-header success PNGs were checked for cart rails on **both**
displays, rejecting blank or shifted-header false successes.

The main-only viewport is640×480, so that header lies outside it. Its corrected
three-way detector uses x50–550,y200–350, with the same RGB predicate:

| Lit pixels in primary PPM | Classification |
|---:|---|
| 1500–1700, around the measured1599 | ok, only with DSI-2 active and DPMS on before/after capture |
| >4000 | STALL, with those same output checks |
| Anything else, including0 | UNKNOWN; exclude and repeat |

[Positive Loading control](assets/2026-10-27-single-detector/loading-positive.png)
measured **42438**. It visibly says READING CARTS. Taken at13:57:46.114 during a
separate early cold-start control sweep, its portal was con_id20, rect
(0,0,640,480), fullscreen_mode1 in both bounding tree reads; DSI-2 was on and
DSI-1 off. It is a real single-display portal viewport, not a crop of a spanning
portal, and it is not counted as a120s measurement.

[Positive attract control](assets/2026-10-27-single-detector/attract-positive.png)
measured **1599**. A blank startup control measured0, validating UNKNOWN rather
than treating absence of pixels as success.

An earlier two-way single-v2 run3 reported0. Its primary PPM was valid but black;
[its immediately later full PNG](assets/2026-10-27-single-detector/run3-full.png)
shows attract and1599 in the region. The hint has a1400ms blink animation,
consistent with the different frames. Historical DPMS was not recorded, so
that sample stays UNKNOWN and is excluded. The fresh single-v3 series replaced
it: all six **original PPM strips** recounted1599, all12 DPMS checks passed, and
all full PNGs showed cart rails. Later PNG hint phases are not substituted for
the primary strip measurement.

A preceding operator capture also failed because grim's `-o` and `-g` are
mutually exclusive. That was a CLI error, not a device timeout. Strip capture
now uses `-g` only; full main-display PNGs use `-o DSI-2` only. The failed sample
was synced and excluded, and the series was restarted.

### Runtime safeguards, interruptions and evidence

- Only the owner's original preamble was used in the final sample:
  `output DSI-2 power on; output DSI-1 power on`. No output enable/position changes.
- At13:25:19 `systemctl stop korri-display-idle` succeeded and reported inactive.
  It stayed runtime-stopped throughout; a final check after14:50:58 confirmed
  inactive. This avoids the separately tracked idle-power-off hang, not a
  persistent service change. Each attempt finished with `journalctl --sync`.
- Sway, unit, journal and screenshot commands were bounded. A timeout would
  stop the serial coordinator; SSH used keepalives and no reconnect loop.
- The first interrupted investigation lost run5 to the owner's short power-key
  shutdown, logged at05:17:25 (#287), not an established experiment effect.
  Its baseline3/6 and interrupted floating2 ok/2 stall are not pooled into the
  final floating sample.
- The13:14:21 attempt never connected: after the hard reboot `/tmp/rgds-round2`
  survived but `/tmp/acc2` did not exist. The previous journal ended13:12:36,
  after swayidle's display power-off13:11:38. The owner tracks that hang separately.
- Local logs/scripts are under `/tmp/rgds-round3/`, validated captures and JSON
  under `/tmp/rgds-round3/final-evidence/`; originals are under device `/tmp/acc3/`.
  Earlier detailed command audits are preserved under
  `/tmp/korri-main-only-brief-archive/`, not retained as obsolete runtime guidance.

## Implementation cut

`host/display_placement.rs` selects only main for catalog facts and uses normal
output fullscreen, not a bounding rectangle. It powers main on, moves/focuses
the unique kiosk-unit-owned window on main, then powers unused displays off in
the same geometry batch. The blocking power-off tail cannot precede portal
focus. Overlay geometry remains its existing floating free-display recipe.

`host/session_state.rs` retains one committed catalog-facts cache keyed by live
kiosk identity. Both requested scopes use it without extra effects. The former
readiness lifecycle, timer, post-response HTTP stream and helper module are
removed. There is no duplicate readiness/placement state. Native game/overlay
placement invalidates the catalog cache as before.

The distinct-size margin,250ms preparation interval, global portal activation,
500ms readiness reflow and their dedicated tests were deleted. They existed to
repair spanning. No replacement heuristic, compatibility branch or fake commit
fence is added. Game multi-pane global fullscreen and the local game-floating
opacity workaround remain valid and are not portal spanning.

The native Sway generator powers non-main displays off at compositor startup;
logical display configuration and touch association remain for later game and
overlay use. Single-display generated configurations stay byte-for-byte unchanged.

The independent status-enrichment mutex fix remains documented in
[session status contention](2026-10-26-session-status-contention.md).
The opacity and dim-before-freeze investigations remain separate because their
game effects are still current. Obsolete spanning-restore briefs are folded here.

## Clean-candidate acceptance — pending staged build

The parent will stage this committed main-only candidate with Pico, reboot,
select the bundle/portal, and hand the device back. No deployment or reboot is
authorized for this child. The current device binary cannot establish that
removed compiled mechanisms are unnecessary; its runtime recipes always followed
them. Do not relabel the earlier6/6 control as clean-build acceptance.

| Candidate test, both old mechanisms absent | Result |
|---|---|
| Cold kiosk start ×6 | Pending parent staging |
| Play → Leave (DSI-1 overlay) → End (DSI-2 catalog) ×6 | Pending parent staging |

Use the three-way one-display detector and positive controls above, audit
inputd events, sync journals, keep display-idle stopped, and stop without retry
if the device becomes unresponsive. A measured failure gets a subsequent commit
restoring only a mechanism that the numbers show is needed, not a speculative
carry-over of the spanning workarounds.

## Checks on the clean candidate

- Red: the old all-scope rectangle and fake-Sway catalog tests returned two
  displays instead of one. The new RG DS startup-power assertion also failed
  before the native generator change.
- Green:181 focused korrid tests (display placement, session state, compositor
  focus and public launch-handoff routes). All-target check, Clippy and format
  passed; existing warnings remain.
- Green:41 inputd library tests, all-target check, Clippy and format.
- Green: Nix korri-linux-host-module, rgds and one-display-unchanged checks.
  The last check verifies complete generated Sway bytes on six existing
  single-display devices. Touched Nix files were formatted.
- These checks exercise native subprocess Sway/systemd boundaries. They do
  not substitute for the12 pending on-device clean-build cases above.
