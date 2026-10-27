# Chromium 153 catalog configure settling — 2026-10-26

> **Correction:** same-size phase-1 preparation could leave Chromium's visible
> main-thread presentation stalled after catalog restore. It is superseded by
> [distinct-size preparation](2026-10-27-distinct-size-catalog-preparation.md):
> the RG DS requests 640×900 before global 640×960, retaining the 250 ms settle.
> The near-startup device repair stayed live; the integrated first configure
> still needs acceptance. The historical tests below did not prove liveness.

## Correction

The single-batch tiling/global restore in `e391f8f35` did **not** fix the device's
floating-overlay → End transition. The earlier manual repair experiment did not
establish that the same commands would work when combined into that transition.
Keep the earlier status-catalog mutex fix; replace only catalog geometry restore.

## Version-matched reproduction

The flake locks nixpkgs revision
`4975466d324710c576dc11ad614684e6bd8cad8e`. Evaluating that input for
`x86_64-linux`, with unfree packages enabled, resolves Chromium to:

```text
/nix/store/agamzskapmh5018k98bqfwrrb738bgpv-chromium-153.0.8010.52
```

This exact wrapped binary ran on isolated headless Sway 1.12 with two 640×480
outputs at (0,0) and (0,480), Pixman, Wayland/Ozone, `--kiosk`, and `--disable-gpu`
(the latter also matches RG DS configuration). CDP `/json/version` independently
reported `Chrome/153.0.8010.52`. Chromium's sandbox was not disabled.

**Verified locally:** three overlay → e391-restore cycles rendered correctly,
without the device's +240 offset. This remains a non-reproduction on x86, even
with the matching Chromium version. Do not claim the version alone explains
an aarch64-only defect.

`WAYLAND_DEBUG=client` showed the non-failing old transition sending
`set_window_geometry(0,0,640,960)`, `ack_configure`, then `wl_surface.commit`.
It did not show stale 480-high window geometry with a 960-high buffer. The
corresponding device protocol trace remains unknown.

The proposed two-phase restore also passed five cycles. Grim pixel checks on
both outputs verified red at global y=0, green at y=80, blue at y=480, and yellow
at y=720. Final state was a tiling `con`, fullscreen mode 2, rect (0,0,640,960).
Restore command duration including the explicit interval was 255.12–256.11 ms
on this development host, excluding the later captures.

## Parent's RG DS experiment

The parent ran `/tmp/rgds-serve/twocall-test.sh` against system yywx60j4:

- A fresh play → Leave → End using e391 was shifted: attract carts at y 290–390.
- The two-call candidate below, separated by 250 ms, repaired that already-shifted
  960-high state: carts returned to y 50–150, text to about y=240 per display.
- Repeating the candidate on the corrected state kept it correct.

The preparation had the same 640×960 size as the preceding fullscreen rect.
Thus a real *size change* is not proven necessary. A separately processed
non-fullscreen configure appears to be the important difference. This is an
inference, not a complete Chromium root-cause proof. Captures are
`/tmp/rgds-acceptance/two-*.png`.

## Implementation

Only multi-output catalog placement (`all`, no game placement) changes.

1. Keep the existing output-power batch.
2. Send a preparation batch: exact portal `fullscreen disable; floating enable;
   resize` to the target catalog extent; `move` to its origin. Do not combine
   global fullscreen with this configure and do not focus a different window.
3. Wait `PORTAL_SETTLE_INTERVAL`, a named 250 ms constant.
4. Re-read the tree and require the same unique unit-owned portal node. This is
   an **identity recheck, not a commit fence**. Refuse activation if it changed.
5. Send a separate activation batch: `fullscreen disable; floating disable;
   fullscreen enable global; focus`.

The superseded preparation-time tiling/global commands are removed. Final
activation still explicitly exits fullscreen before changing floating state.
Each mutation is sent once. Preparation failure skips waiting and activation.

All catalog callers use this same path. Gameplay overlay placement, game
Return, and single-display placement do not wait. Existing exact unit/PID
selection remains authoritative; no app-id fallback, program, service, schema,
or compositor/Chromium patch is added.

`PlacementTransition` now logs catalog placement, including `total_ms` and
`phases_ms.portal_settle`. Geometry time sums the two command batches; tree time
includes identity revalidation. Leave/Return lines carry `portal_settle: 0`.

## Why a bounded interval, not an alleged commit signal

Read the tested Sway 1.12 sources at
`/nix/store/2j7grygxd5da5r214vzz3y8b54rr739k-source`:

- `sway/tree/container.c:1015` emits `window::floating` while changing the
  container, before the later asynchronous configure transaction has completed.
  Fullscreen events at lines 1217, 1243 and 1284 likewise describe state changes.
  They are not xdg client-commit acknowledgements.
- `sway/ipc-json.c:596–607` builds `window_rect` from pending content dimensions;
  `geometry` remains natural size. A second `get_tree` cannot prove that a client
  acknowledged or committed those dimensions.
- `sway/desktop/transaction.c:820–875` configures the client and waits internally.
  `set_instruction_ready`, lines 887–911, handles acknowledgement and transaction
  progress but emits no IPC window-commit event.
- `sway/ipc-server.c:918–924` explicitly rejects `IPC_SYNC` with
  `success:false`; it does not provide an alternative synchronization fence.
- `sway/server.c:697` gives the default transaction timeout as 200 ms.

Existing Sway IPC therefore exposes no trustworthy client-commit fence for this
operation. The parent approved the bounded 250 ms settle, which exceeds the
compositor's default transaction interval and repaired the actual device state.
It is not a guarantee that a wedged or heavily delayed client committed. Do not
add window-event polling or pending-tree polling and label either as proof.

In the headless two-phase trace, for example, preparation sent a 640×960
non-fullscreen configure at 20:45:08.609939. Chromium set 640×960 window geometry
and committed by 20:45:08.617187. The final fullscreen configure followed at
20:45:08.863181, after that intermediate commit.

## Tests, cost and remaining acceptance

Fake-Sway tests pin the separate preparation/activation calls and intervening
identity read. They reject activation after portal replacement and after a
refused preparation. Existing fullscreen-state order and overlay batch tests
remain, and the overlay path records no settle delay.

The cost is a 250 ms sleep, one extra geometry IPC and one identity tree read on
catalog restore. The native transition mutex remains held: releasing it would
allow game Return to race the final global portal activation. Ordinary Home and
game Return do not take this catalog path, but a concurrent catalog mutation
can therefore delay them. Catalog restoration is slower in exchange for the
verified intermediate configure. No unbounded poll or mutation retry is added.

Fresh overlay → End with the integrated new build still needs RG DS acceptance.
The parent tested the candidate on an already-shifted state, not as a substitute
for e391's original End batch. Local evidence, scripts, pixel checks, exact
version metadata, and Wayland trace are in `/tmp/korri-chromium153-settle/`.
No RG DS access or deployment was performed by this child agent.
