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

### Historical hint-strip detector and positive controls

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

## Clean-candidate device acceptance — completed 2026-10-27

The parent staged main-only `dfb38ea55` with Pico at scratch head `2e75c8f3d`
(design head `ac44c829a` differs only in comments/docs). The device booted about
15:38 UTC on unchanged system1wqzjsag, selected bundle88vxm5fy and portal
yjkixvgz. This child performed no update or reboot. The hardware tests exercise
the new bundle's korrid path; the Nix startup-generator change is covered by
module checks, not a changed system closure in this measurement.

Both spanning mechanisms were absent. There were **no corrective Sway move,
resize or fullscreen commands** in these measurements: the product code alone
placed the portal. The operator preamble was only the original power-on batch.

| Clean candidate, both mechanisms removed | Attract at+120s | Stalls |
|---|---:|---:|
| Cold kiosk starts | 6/6 | 0 |
| Play2048gb → Leave (DSI-1 overlay) → End (DSI-2 catalog) | 6/6 | 0 |

**Keep both mechanisms removed.** These runs do not justify restoring either
preparation or readiness timers. This is six observations per path, not a
universal guarantee or a Chromium root-cause proof.

Cold samples completed by16:00:38 UTC. Cycle samples1–4 completed earlier;
the final two completed at16:18:50 and16:21:18 UTC. End measurement deadlines
were anchored to CLI End completion, not to later placement polling. All six
overlay trees were focused at(0,480,640,480), fullscreen_mode0 on DSI-1. At
classification every portal was the only mapped PID-bearing window, focused,
fullscreen_mode1 at(0,0,640,480). DSI-2 was DPMS-on and DSI-1 off in bounding
output reads. Saved journal windows had no inputd_policy_match contamination.
Each attempt finished with journalctl --sync.

### Correction: the blinking hint was not a valid attract detector

The historical y200–350 hint strip produced zero when PRESS ANY BUTTON was in
its blink-off phase. That did **not** mean the whole display was black. Full
PNGs for cold6 and cycle2–4 show the colored attract cart row at y50–150. The
owner independently reviewed and accepted those captures as attract. The
extra repeated cold attempt also showed attract; it is not used to inflate the
requested six-run result.

Representative zero-hint capture states, checked from saved metadata:

| Capture UTC | DSI-2 / DSI-1 DPMS | Portal | Other mapped windows | Full-frame row/header counts |
|---|---|---|---|---|
| Cold6,15:58:31.905 | on / off | focused, fs1, rect0,0,640,480 | none | 50732 /0 |
| Cycle2,16:05:32.284 | on / off | focused, fs1, rect0,0,640,480 | none | 50968 /0 |
| Cycle3,16:07:58.945 | on / off | focused, fs1, rect0,0,640,480 | none | 48800 /0 |

The saved ±10s journals show the *following* automatic attempt after those
captures: cold6's next kiosk stop at15:58:32.801; cycle2's next game unit start
at16:05:36.002; cycle3's next game unit start at16:08:02.644. Those later
transitions do not explain away the PNGs captured earlier. At capture time
there was no leftover game window or black main-output evidence.

The initial repetition loop was stopped too late: it had completed cycle4 and
launched another game. The owner ended that interrupted launch; it is excluded.
The final two cycles used a fixed two-iteration script with no automatic retry
loop. The first four accepted cycle PNGs plus those two make the six above.

### Current detector: cart row plus library header, not blinking text

For the final two cycles, both regions are counted from **one full640×480
PPM frame**, using RGB sum>90. The same masks were applied to saved full PNGs
when reclassifying the earlier captures:

- Cart row: x0–640,y50–150.
- Library header: x16–256,y10–50; it excludes the cart row starting at y50.
- Header>1000 means Loading/Ready remains (STALL at+120s).
- Row>20000 and header<1000 means attract (ok).
- Otherwise UNKNOWN; blank0/0 cannot pass. DSI-2 must be active and DPMS-on
  before and after capture, with no leftover game window and the portal focused
  fullscreen on main. These checks remain independent of pixel classification.

Fresh controls on this staged build:

| Frame | Cart-row pixels | Header pixels |
|---|---:|---:|
| [Loading positive control](assets/2026-10-27-single-detector/main-only-loading-positive.png) | 3648 | 2445 |
| Attract control, earlier cycle2 PNG | 50968 | 0 |
| Attract control, earlier cycle3 PNG | 48800 | 0 |
| Final cycle5 primary PPM | 49944 | 0 |
| Final cycle6 primary PPM | 49916 | 0 |

The fresh Loading control at15:45:21.144 visibly says READING CARTS. Its full
single-display viewport had con_id10, fs1 and rect0,0,640,480 in bounding tree
reads; output metadata showed main on and other off. It was captured without
any scene/geometry override. Its old hint-region count was42438, retained only
for comparison with the superseded detector. The new detector uses3648/2445.
[Final cycle6 full PNG](assets/2026-10-27-single-detector/main-only-attract-row.png)
shows attract; its row count differs slightly from the preceding primary PPM
because the cart rail drifts, but classification is unaffected.

### Handoff and remaining limits

All measurement processes ended. At16:23:38 UTC this child started
korri-display-idle, confirmed active, and synced the journal. The device was
handed back with no queued tests. No further device access followed.

The CLI warned that offline saves were not checked and could split; no save
policy or permission was bypassed. No new program, plugin or bundle was changed
by this child during acceptance. The default journal and native ownership/focus
checks remained in force.

Raw scripts/logs and original controls: `/tmp/rgds-main-acceptance/`. Final
fixed-cycle scripts, captures, PPMs, trees, output metadata and journal windows:
`/tmp/rgds-main-final/evidence/` locally; device originals `/tmp/main-acc/`.
The owner also retained copies under `/tmp/rgds-acceptance/main-acc/`.
Software remains `dfb38ea55`; this acceptance record changes documentation only.

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
- These checks exercise native subprocess Sway/systemd boundaries and preceded
  the12 completed on-device clean-build cases above.
