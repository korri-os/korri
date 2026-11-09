# Stream at the client's shape

**Status: implementation, 2026-11-09. Custom-mode hardware gate passed on aka
at 08:30Z. Sunshine capture and encoding verification is still pending.**

## Problem

Every stream from aka reaches the client as 1920x1080 (16:9). A phone such as
the Galaxy Z Fold 7 (inner screen about 1.2:1), the Retroid Pocket Mini V2
(about 1.15:1) or a 16:10 screen shows black bars, even when the game itself
could fill any shape.

Verified on aka, 2026-11-09:

- aka's only display is `HDMI-A-1`, an HDMI dummy plug ("Ariel FH-HD4K"),
  1920x1080@60. The device marks it `unwatched` (commit `a0c82b16b`,
  CONTEXT.md "Unwatched display").
- Sunshine captures it with `capture=kms` and logs `[wayland] Resolution`
  once per stream session, so it reads the output size again at each session
  start.
- The game's gamescope gets `--output-width 1920 --output-height 1080` from
  `LaunchComposition.display` (`launcher/linux_plugin.rs`), which comes from
  the live Sway output (`host/compositor_focus.rs` `display_from_outputs`).
  `Placement.width/height` comes from the static `KORRID_DISPLAYS` facts.
- The far korrid prepares and starts the game **before** the viewer connects
  to Sunshine (`upstreams.rs`: `prepare_stream`, then `viewer.start`). So
  Sunshine's per-session client size arrives too late to size the game.

## Decision

The streaming device tells the far korrid the picture size it will show, in
the prepare call. When the far device's main display is unwatched, the far
korrid sets that display to the client's shape before the game starts, and
puts it back when the shell takes the display again.

1. **Treaty.** `SessionPrepareRequest` gains `viewerDisplay:
   Option<PluginDisplay>` (`{ width, height }`). Grounding: `PluginDisplay` is
   the existing launch type for "the size the game draws at"; the value is the
   stream size each viewer already computes for Moonlight (Linux:
   `linux_viewer/production.rs` renders Moonlight settings from the local
   display size; Android: `Game.java` `prefConfig.width/height`).
   Absent means "no preference": today's behaviour.
2. **Viewer reports its size.** `LinuxStreamViewer` gains
   `fn display(&self) -> Option<PluginDisplay>` (default `None`). The Linux
   production viewer returns the size it gives Moonlight. The Android adapter
   asks Java (`KorriStreamViewer`) for the stream size Moonlight will request.
   **Owner decision, 2026-11-09:** on Android that size is the screen's own
   resolution at stream start (default display mode, long side first, with
   Game's portrait inversion), not the Moonlight resolution setting. The
   viewer passes the same size to `Game` as `EXTRA_WIDTH`/`EXTRA_HEIGHT`, so
   the far display and the stream request agree. This is temporary: stream
   size is meant to become a cascading setting.
   `upstreams.rs` passes it to `prepare_stream`. A joiner (`stream_play`)
   sends nothing and keeps the starter's shape.
3. **Display facts.** `KORRID_DISPLAYS` entries carry `unwatched` from the
   existing Nix option (`korri-linux-host-core.nix`). `Display` gains
   `unwatched: bool`.
4. **Shape.** Use the client's own size. **Owner decision, 2026-11-09:** the
   dummy plug takes virtually any mode, so there is no 1080p pixel budget; an
   earlier version of this brief capped the size at the configured mode's
   area. Only the 4096-per-side limit scales a size down, keeping the aspect
   ratio. Round both sides down to multiples of 8: CVT uses eight-pixel
   horizontal granularity, and aligning both sides is safer for the encoder.
   For example, a Galaxy Z Fold inner screen at 2448x1848 stays 2448x1848, and
   2176x1812 becomes 2176x1808. Cost: the game renders and Sunshine encodes at
   the client's full resolution.
5. **Apply.** Only when a `viewerDisplay` is present, the placement is one
   pane on the main display, and that display is `unwatched`. korrid runs
   `output <name> mode --custom <W>x<H>@60Hz` through its existing Sway
   control, confirms the output reports that size, then composes the launch
   with that size for both `Placement` and `display`. If Sway refuses or the
   size does not confirm, the launch goes on at the configured mode and
   carries a launch warning. A watched display is never resized.
6. **Restore.** When the shell takes the main display back (surface placement
   with no game, `display_placement::place_surface`), korrid sets an
   unwatched display back to its configured mode if it differs.

## Not in scope

- Changing shape while a game runs. A game fixes its picture size when its
  graphics start; every launch starts fresh at the client's shape.
- The Hookshot patch for screens narrower than 16:9 (separate work, after
  this).
- Watched displays and dual-pane placements.
- Known limit: composition, including the limits helper, runs under the session
  lock, as before this change.

## Hardware gate

The owner reported these results on aka at **2026-11-09 08:30Z**:

- `swaymsg -- output HDMI-A-1 mode --custom 1576x1314@60Hz` is accepted
  by the dummy plug. `get_outputs` reads back `1576x1314@60000`.
- Custom `1920x1200@60Hz`, `1600x1200@60Hz` and `1576x1314@30Hz` work too.
- Restoring with `mode 1920x1080@60Hz` works.
- Plain `mode 1600x1200@60Hz`, without `--custom` and not a listed mode,
  returns success but leaves the output at 1920x1080. Read-back confirmation
  is required and must remain in place.
- On the command line, `swaymsg` needs `--` before
  `output ... mode --custom`. Without it, `swaymsg` reads `--custom` as its
  own flag. `SwaymsgCompositorControl::command` already passes `--`.

The custom-mode acceptance gate has passed. Sunshine capturing and encoding
at the shaped size is **not yet verified**. Check that after deploy with a
real stream.

## Implementation clarification

The configured refresh was not present in `KORRID_DISPLAYS`. The supervisor
resolved this on 2026-11-09: carry the existing Nix `display.mode` string as
`mode` alongside its derived width and height. Restore that native mode
verbatim. Custom modes use its `@...` refresh, rather than assuming every
device is configured at 60 Hz. No second authored mode is introduced.
