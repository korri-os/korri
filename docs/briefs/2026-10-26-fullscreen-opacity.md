# Fullscreen opacity investigation — 2026-10-26

## Result

Verified: Sway 1.12 accepts `opacity 0.4` but does not dim a fullscreen leaf
view. This reproduces with both Xwayland and Wayland clients in headless Sway.
It is not specific to the RG DS screenshot path.

Verified workaround: disable fullscreen, float the same committed rectangle
without borders, then set opacity, in one command batch. Restore tiling and
fullscreen with the normal game placement/focus batch on Return or rollback.
No runtime program, service, or compositor patch is added.

This workaround is scoped to multi-output Home, where the paused game remains
visible beside the portal. Single-output presentation remains unchanged. Keep
it local to `HostSessionControl::game_opacity`; the indicator-surface spike may
replace it.

## Experiment

Isolated headless Sway 1.12, wlroots 0.20.2, Pixman renderer, Xwayland 24.1.14.
Two outputs, each 640×480, at (0, 0) and (0, 480), black backgrounds, no borders.
The clients ran sequentially fullscreen on HEADLESS-1:

- Wayland: foot 1.26.0 with a white background.
- Xwayland: an Xlib white window, repainted on configure/expose.

After fullscreen settled, SIGSTOP paused each client before the opacity and
floating experiments. The client stayed paused through the restoration capture.
`grim -o HEADLESS-1` captured the composited output. IPC readback confirmed the
floating and restored rectangles remained (0, 0, 640, 480). The floating node
had `fullscreen_mode: 0`; the restored tiled node had `fullscreen_mode: 1`.

| Capture, center pixel RGB | Wayland | Xwayland |
|---|---|---|
| Fullscreen, opacity 1 | 255, 255, 255 | 255, 255, 255 |
| Fullscreen, opacity 0.4 | 255, 255, 255 | 255, 255, 255 |
| Floating same rectangle, opacity 0.4 | 102, 102, 102 | 102, 102, 102 |
| Restored fullscreen, opacity 1 | 255, 255, 255 | 255, 255, 255 |

The Xwayland white window filled the complete capture: mean RGB changed from
(255, 255, 255) to (102, 102, 102), then back to (255, 255, 255).

Workaround batch (the rectangle in production comes from the exact window's
Sway `rect`, not its initial natural `geometry`):

```text
[con_id=N] fullscreen disable; [con_id=N] floating enable; [con_id=N] border none; [con_id=N] resize set width 640 px height 480 px; [con_id=N] move absolute position 0 px 0 px; [con_id=N] opacity 0.4
```

Five repeated trials per client, measured on the development host, not RG DS:

| Metric | Wayland median (range), ms | Xwayland median (range), ms |
|---|---|---|
| Old opacity-only IPC | 1.11 (0.72–1.26) | 0.87 (0.83–0.98) |
| Workaround IPC batch | 1.26 (1.00–1.61) | 1.51 (1.15–1.67) |
| Batch start to first dim grim capture | 16.38 (16.00–32.77) | 16.17 (15.22–34.25) |
| Restore IPC batch | 1.30 (1.09–1.57) | 1.22 (0.96–1.84) |

Difference of IPC medians: +0.15 ms Wayland, +0.64 ms Xwayland. The capture
measurement includes grim and frame scheduling; it is an upper bound on when
the changed pixels became available, not a panel latency measurement. No extra
Sway process or tree read is added to production's existing `game_dim` phase.

Local evidence: `/tmp/korri-opacity-investigation/` contains `probe.py`,
`xwhite.py`, `sway.conf`, `repeat-results.json`, PNG captures, and IPC tree
snapshots. These local files are not deployment artifacts.

## Source explanation

Read the sources of the exact tested Nix binaries. Sway source:
`/nix/store/2j7grygxd5da5r214vzz3y8b54rr739k-source`.
wlroots source: `/nix/store/r2zcb3d3dgmz09qjaqsy945inal2h41r-source`.
The tested Sway executable resolves to
`/nix/store/jh2y4bkbdd6a0dkpw6ryz0j8cgy6hg7k-sway-unwrapped-1.12/bin/sway`.
Its derivation uses that Sway source and wlroots 0.20.2; its three Nix patches
change configuration/path handling, not this rendering code.

- **Sway `sway/tree/container.c:96–97`** attaches the container descriptor to
  the container's scene tree.
- **Sway `sway/desktop/transaction.c:492–513`**, `arrange_fullscreen`, takes
  `fs->view->scene_tree` for a leaf view, disables `fs->scene_tree`, and
  reparents the view directly into the fullscreen layer. The opacity-bearing
  container tree is no longer its ancestor.
- **Sway `sway/commands/opacity.c:40–42`** stores `con->alpha`, configures the
  container scene tree, and reports command success. Success does not prove
  that fullscreen pixels changed.
- **Sway `sway/desktop/output.c:212–248`** skips disabled nodes, picks up alpha
  from container descriptors, and propagates it to scene buffers. The
  fullscreen leaf view bypasses that descriptor. The output pass starts with
  opacity 1 at **line 278**.
- **Sway `sway/tree/output.c:120–121`** creates an opaque black fullscreen
  background. **`sway/desktop/transaction.c:598–608`** enables/sizes that layer
  and arranges its fullscreen view. A black fullscreen background exists; the
  missing alpha propagation, not a white backdrop, explains this experiment.
- **wlroots `types/scene/wlr_scene.c:269–281`** excludes non-unit opacity from
  opaque-region reporting. **Lines 1541–1547** pass buffer opacity to the
  composited texture render. **Lines 2440–2448** attempt direct scanout only
  when the render list has one entry. Headless/Pixman does not exercise DRM
  direct scanout; the failure already reproduces without it.

## Verification and limits

Fake-Sway session tests verify freeze → same-rectangle floating dim → panel
power → portal focus, repeated cycles, full restoration on Return/rollback,
stop/graceful End cleanup, and refusal handling. The fixture deliberately has
natural geometry 320×240 but committed rect 640×480.

The remaining check is RG DS acceptance of this workaround. Do not infer its
panel latency from host IPC timings. Opacity blends with the compositor's
existing backdrop; a dedicated translucent black indicator surface can give
the later label spike explicit control of that backdrop.
