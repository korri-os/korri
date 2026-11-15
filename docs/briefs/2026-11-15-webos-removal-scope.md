---
date: 2026-11-15
topic: webos-removal
artifact: scope
status: done; owner decision 2026-11-11 (issue #433)
---

# Dropping webOS: what went, what stayed

The owner dropped webOS on 2026-11-11, during the design of #391. This brief
records the cut, as `2026-09-16-android-removal-scope.md` did for Android.
The tag `webos-final` marks the last commit on `main` that carried webOS.
Restore any part from that tag; do not bring it back piecemeal from memory.

## Whole-tree deletions

| Path | Files | What it was |
|---|---|---|
| `clients/shell/webos/` | 8 | LG B7 IPK packaging, native launcher, media check, proofs |
| `clients/shell/window-sdl/` | 7 | Slint platform over the TV's `libSDL2-2.0.so.0`, Magic Remote cursor |
| `clients/shell/src/backend/sdl.rs` | 1 | the shell's `sdl` backend (feature `webos`) |
| `clients/slint-spike/` | 39 | throwaway Slint inspector; its webOS SDL half, SDK and runtime overlay scripts |
| `clients/webos-embedded-viewer-spike/` | 12 | NDL media test and ELF symbol audit for the embedded viewer |
| `nix/webos/` | 1 | `ipk.py`, the IPK writer |
| `services/korrid/src/bin/webos-korrid.rs` and `webos-korrid/` | 4 | korrid for the TV: evdev input, devmode permission repair |
| `services/korrid/src/bin/webos-owner-sign.rs` | 1 | workstation tool to sign the TV's owner-binding request |
| `services/korrid/src/webos_owner_sign.rs` and `_tests.rs` | 2 | the library side of that tool |

`clients/slint-spike/` went whole. Its desktop X11 half is not webOS, but it
was a throwaway spike whose question the native shell answered, and its
window came from `window-sdl/`.

## Edits in files that stay

- `flake.nix`: the `shell-webos-*` and `slint-spike-*` apps are gone.
- `clients/shell`: the `webos` feature, the `korri-window-sdl` dependency and
  its clippy and fmt lines in `shell-check` are gone. The window systems are
  now `desktop` (default) and `android`.
- `services/korrid/src/lib.rs`: the `webos_owner_sign` module is gone. The
  embedded entry points it used (`start_local_shell_server`,
  `local_server_capability` and others) stay; Android and tests use them.
- Comments and READMEs that named the webOS target.

## What stays on purpose

- Older briefs and ADRs that mention webOS. They are history.
- Evidence records (`clients/shell/evidence/`, `clients/android/evidence/`)
  that list a webOS check they ran at the time.
- `nix/stream-cadence/README.md` names an LG TV as a display on aka. That is
  hardware on a Linux host, not webOS support.

## The unlanded branches

The embedded C viewer never landed. It lives on the `wip/webos-*` branches.
`2026-11-06-webos-milestones.md` records the last state. The branches stay on
the remote; this cut deletes none of them.

A read of every `wip/webos-*` branch against `main` found nothing to keep
for #391. The viewer's protocol, TLS pinning, audio and GameStream handshake
are C in `clients/webos-viewer/`, built for NDL, PulseAudio and SDL on the
TV. The korrid side (`webos_viewer.rs`, the `localViewer` watch push,
`subscribe_local_gamepads`) only supervises that C child.

One platform-neutral fix sits on `wip/webos-viewer-audio-diag` in
`27590d831`: a later `HostLaunchFailed` (exit 143) no longer erases the
known cause of a failed launch in `surface/launchables/state.rs`. Issue #488
tracks harvesting it on its own.

## Issues

Closed as webOS-only: #370, #373, #386, #420. #369 now names Android only.
