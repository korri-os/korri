# Distinct-size catalog preparation — 2026-10-27

> **Correction after integrated device acceptance:** `5a900abeb` fixes the
> post-game catalog restore, but a single startup restore still stalls cold
> kiosk presentation for minutes. The successful near-cold probe was a second
> restore after the automatic one, not proof that the first one was safe.
> [Post-reply reflow](2026-10-27-portal-readiness-reflow.md) preserves that
> two-restore sequence once per verified kiosk instance.

## Correction

The same-size two-phase restore introduced by `ee8ee4123` repaired the observed
+240 offset but was not a safe general presentation workaround. Device evidence
on system qgw1i5hd (`d59e9385c` plus Pico) shows visible main-thread updates
stalling after cold catalog restoration while compositor-only animation keeps
moving. A real 640×900 resize followed by global fullscreen restores the current
page. The earlier same-size preparation must be replaced, not retained as an
alternative path.

The precise Chromium Viz/LocalSurfaceId cause remains an inference. The JS page
is alive, but its visible main-thread commits stall. Do not describe this as an
input policy lockout or claim that a Sway command acknowledgement proves client
presentation is live.

## Change

Every multi-output `all` catalog restore uses the existing shared producer:

1. Power the requested outputs using the existing power batch.
2. Disable portal fullscreen, float it, and request the catalog width at
   **target height minus 60 pixels**, at the catalog origin.
3. Retain the named **250 ms** settle interval and exact portal identity recheck.
4. Separately disable fullscreen, disable floating, enable global fullscreen,
   and focus the same portal.

The named `PORTAL_PREPARATION_HEIGHT_MARGIN` is 60 pixels because the owner's
successful real-device probe used 640×900 before returning to 640×960. The
margin is capped at height minus one on tiny canvases, so the preparation stays
positive and distinct; a one-pixel-high catalog cannot supply a smaller valid
height and is rejected before any mutation. Normal RG DS preparation is exactly
640×900, not an arbitrary alternative size.

The final surface facts remain the full target rectangles, not the temporary
900-high extent. Startup, End, open/return to catalog, exit/crash and recovery
all use this same `all` placement branch. Game overlay, game Return, dimming and
single-display placement remain unchanged. No new program, permission, native
patch, persisted schema, viewport cache or frontend layout change is added.

Catalog `PlacementTransition` now includes requested `phase1_size`, for example
`Some((640, 900))`, alongside measured `phases_ms.portal_settle`. Requested size
is not a claim that the client committed it when a placement fails.

## Why retain 250 ms

There is still no client-commit fence in the available Sway 1.12 IPC:
`window::floating` and fullscreen events describe pending state; `get_tree`
serializes pending dimensions; `IPC_SYNC` explicitly returns failure. Source
citations are in the preceding Chromium 153 settle brief. No pending-state poll
is relabelled as an acknowledgement.

The 250 ms interval exceeds Sway's default 200 ms transaction timeout, but is
not a guarantee for a wedged client. The parent specifically tested the changed
size with 250 ms near a cold kiosk start and asked to retain that value rather
than impose the earlier probe's three-second delay.

**Verified by the parent on RG DS:** `/tmp/rgds-serve/cold-900.sh` restarted the
kiosk at 02:27:29. After d59's automatic catalog restore at 02:27:31, the parent
immediately applied 640×900, waited 250 ms, then restored global fullscreen.
Without input, the page progressed from Loading through Ready to attract by
02:29:41 and remained animated at 02:30:52. Without the changed-size preparation,
the comparison restart stayed visibly on Loading for at least 3.5 minutes.
Evidence: `/tmp/rgds-acceptance/cold900.log`, `cold900-1.png`, `cold900-2.png`.

This validates the repair shortly after cold startup, not the integrated build's
very first automatic configure. That final acceptance check remains necessary.

## Headless Chromium 153 checks

Used the flake-pinned x86 Chromium 153.0.8010.52 at
`/nix/store/agamzskapmh5018k98bqfwrrb738bgpv-chromium-153.0.8010.52`, Sway 1.12,
Pixman, two 640×480 outputs, and the product's Wayland/kiosk/disable-gpu flags.
Each case had a fresh Chromium profile. The probe separately displays CSS
animation and a JS interval which changes a solid heartbeat pixel, so a CSS
animation alone cannot satisfy the main-thread presentation check.

| Cold case | Preparation height | Restore IPC duration including settle | Main-frame captures over 60 s |
|---|---:|---:|---|
| Same-size control | 960 | 263.65 ms | 30, JS heartbeat changes visible |
| Distinct-size A | 900 | 256.86 ms | 30, JS heartbeat changes visible |
| Distinct-size B | 900 | 262.10 ms | 30, JS heartbeat changes visible |

The real-size restores started about 0.83 s and 1.21 s after process launch.
The device presentation stall did **not** reproduce locally, including in the
same-size control. These are limited synthetic-page results, not proof that
Pico or the aarch64 device cannot stall. Layout pixels and final 640×960 global
bounds passed. WAYLAND_DEBUG records 640×900 geometry followed by acknowledgement
and commit before the final 640×960 configuration. IPC return duration excludes
later client presentation/capture scheduling.

Local scripts, profiles, protocol traces, results and captures are under
`/tmp/korri-real-resize/`. They are not deployed programs.

## Tests, costs and remaining work

Fake-Sway tests pin the 900-high preparation, its separate activation, and a
phase-1 size different from an already-full-sized portal. They verify that
published facts remain 960 high, the margin remains positive on small canvases,
preparation refusal does not activate fullscreen, and portal replacement still
fails closed. A public controller test pins the info-log size and settle field.

Cost remains the existing temporary floating viewport and 250 ms settle while
native transition authority is held. The temporary extent is now 60 pixels
shorter rather than full-height. It is a workaround, not proof of a Chromium
root cause or a universal timeout bound. First automatic cold restore and
normal play/Leave/End cycles need acceptance on the committed build.

The separate failed dependency/restart issue for `korri-display-idle.service`
was deliberately left untouched. No RG DS access or deployment was performed
by this child agent.
