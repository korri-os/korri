# Stream at the client's shape

**Status: design, 2026-11-09. Hardware gate open: the dummy plug's custom mode is not yet tested.**

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
   `upstreams.rs` passes it to `prepare_stream`. A joiner (`stream_play`)
   sends nothing and keeps the starter's shape.
3. **Display facts.** `KORRID_DISPLAYS` entries carry `unwatched` from the
   existing Nix option (`korri-linux-host-core.nix`). `Display` gains
   `unwatched: bool`.
4. **Shape.** Keep the client's aspect ratio. Scale to the pixel area of the
   display's configured mode (aka: 1920x1080), so GPU and encoder cost stay as
   today. Round both sides down to even numbers. Never exceed 4096 on a side.
   If the client size is already within that area, use it as is.
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

## Hardware gate

Before deploy: on aka with no game running, check that
`swaymsg output HDMI-A-1 mode --custom 1576x1314@60Hz` is accepted by the
dummy plug, that a Moonlight session then logs `Resolution: 1576x1314`, and
that `swaymsg output HDMI-A-1 mode 1920x1080@60Hz` restores it. If custom modes
fail, use the listed mode with the nearest aspect (the plug lists 4:3, 5:4 and
16:10 modes) instead, and record that change here.
