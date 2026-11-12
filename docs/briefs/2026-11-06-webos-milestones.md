# webOS (LG B7) streaming-only Korri: milestones (2026-11-06)

Goal: a streaming-only Korri on the LG B7 (webOS 3.9.3) with the full native
UI. Each milestone ends in something the owner can check on the TV.

| # | Milestone | Owner's check on the TV | Status |
|---|---|---|---|
| M0 | TV healthy | The old spike app reaches the front; D-pad and OK work | Done 2026-11-06 |
| M1 | Real shell shows Zone | Zone at 1080p; D-pad, OK and Back work | Done 2026-11-06 |
| M2 | korrid on the TV, joined to the owner | Zone lists the real games from `haku` | Done 2026-11-06, landed `2d6b2b3b7..649506ea8` |
| M3 | Survives real life | App restart, reinstall and TV reboot keep identity and library | Done 2026-11-07 (real reboot, new boot ID): boot left 777, korrid re-tightened, 38 games from haku, same identity; landed `8af0d4565` |
| M4 | First stream | A haku game in Zone plays on the TV; quitting returns to Zone | In progress: an embedded viewer inside `org.korri.shell` (owner decision, 2026-11-07). M4a (video under a see-through shell) and M4b (native main with the LS2 identity) pass on the TV |
| M5 | Overlay during play | A button opens Korri's overlay over the stream; Resume and End work | Not started |
| M6 | Polish and install | One-command install; the pointer hides on D-pad; smooth scrolling | Not started; cursor is #370 |

## Owner decisions

- 2026-11-07: "We must embed Moonlight into the app. It is not something that
  we reach out to." The webOS stream viewer ships inside the Korri app, as on
  the Linux handhelds and Android. Korri owns its client identity and pairs
  through korrid's existing certificate provisioning. Korri does not use the
  installed `com.limelight.webos` app or a PIN. This replaces the installed-app
  route that `2026-11-07-webos-m4-stream-design.md` researched.
- 2026-11-06: Back at Zone's top screen stays in Korri. It does not exit to
  the TV launcher.

## Facts found on the way

- **Upgrade cleanup.** A webOS IPK upgrade does not remove files the new
  package drops. Uninstall before every install. A stale bundled SDL caused
  the "early hang" (`2026-10-30-lg-b7-native-input-report.md`).
- **App jail.** Per-app UID (6236 for `org.korri.shell`), group 5000,
  supplementary groups 29 44 505 509 777. HOME is the root-owned app
  directory and is not writable. `/tmp` is private and is wiped at boot.
  `/media/developer` is the only persistent read-write mount.
- **Boot resets permissions.** `/etc/init/devmode.conf` line 40 runs
  `chmod -R 777 /media/developer` at every boot while dev mode is on. Line 69
  removes `/media/developer` at boot when dev mode is off. The TV edge
  therefore tightens its own state directory again at start.
- **Input.** Synthetic keys do not reach native SDL apps. LG SDL 2.0.4
  joystick init calls `registerScreenSaverRequest(true)` and fails in the
  jail, so gamepads go through the TV-side korrid reading `/dev/input`.
- **Joining.** No product join flow exists for a new unowned device. M2 used
  the existing offline owner-binding request/import and the dev-only
  `shell-webos-owner-sign` through Amber (NIP-46 `bunker://`). The product
  flow is an open product decision.
- **Discovery.** On a cold start the first catalog read can fail with
  `UpstreamUnreachable` until relay discovery supplies peer endpoints, about
  30 to 60 s. The notice clears by itself. Warm restart: catalog in 7 s.
- **Gamepad (2026-11-07).** The TV-side korrid reads the XInput pad from
  `/dev/input/event2` and feeds korrid's native input; the shell holds no
  input node. The pad drives Zone, but the owner reports a noticeable delay
  compared with the remote's D-pad. An idle pad sends 0 events, so the delay
  is not a noise flood. Under investigation.
- **Media preflight (2026-11-07, read-only).** In the app jail:
  `libplayerAPIs.so.1`, `libAcbAPI.so.1`, `libpbnjson_c.so.2`, GLib 2.40,
  GStreamer 1.8, `libpulse.so.0`, `libNDL*.so.1`. libstdc++ goes up to
  GLIBCXX_3.4.19 only. libcrypto is only 1.0.0, so the viewer must bundle
  its own. The TV holds its own HDMI pipeline; tests must not disturb it.
- **Media (2026-11-08).** ss4s `smp-webos3` cannot load on the B7: the TV's
  pbnjson 2.11 and libplayerAPIs lack four of its imports. `ndl-webos4` (the
  module Moonlight ranks first for webOS 3.5 to 5) resolves completely.
  Its M3 KADP patch is a no-op on this `m16p` SoC. NDL video open fails
  unless the calling process has the app's Luna (LS2) identity. The LS2 role
  binds `exeName` to the appinfo `main` (today `launch.sh`), so no Korri
  binary has it. With the role pointed at the test binary (a temporary
  edit, since restored), NDL opened 1920x1080 H.264, took all 300 frames
  and closed cleanly, twice. The video was not seen on screen, because Zone
  paints an opaque background over the video plane. Design consequence: the
  process that calls NDL must be the appinfo `main` executable. Options are
  a multi-call `main` binary, so the launcher, brain, shell and viewer share
  one exe path, or a viewer inside the main process. The shell window also
  needs see-through regions to show video.
- **M4a and M4b on the TV (2026-11-08).** With the shell's left half
  transparent (8 alpha bits, Wayland opaque region cleared), the TV's own
  display capture (`com.webos.service.tv.capture/executeOneShot`, method
  `DISPLAY`) showed the NDL test pattern in the left half and Zone in the
  right. Graphics composite over the video plane, so the M5 overlay is
  feasible, and the capture includes video, so picture checks can be
  automated. With appinfo `main` = `bin/korri-webos` (landed `e80d3a96f`), the
  LS2 role binds to it, normal launch works (70 games), and
  `korri-webos media-test` opens NDL with no role edit.
- **First real stream (2026-11-08, `wip/webos-viewer-appversion` 651261748,
  host aka).** Zone's PLAY drives the whole chain: streamed launch, embedded
  identity, public serverinfo, native attestation, certificate provisioning
  with no PIN, authenticated serverinfo, app list (`Korri Stream` = 1820356324),
  transport launch, then READY. The TV's display capture shows the game full
  screen under a transparent shell. The pad controls the game, with the same
  lag as in Zone (#373). `app.session.end` stops the exact launch: the viewer
  exits 0, Zone returns with the same PID, and aka's unit deactivates cleanly.
  Fixed on the way: Sunshine's `7.1.431.-1` appversion was rejected, and the
  local cause was masked by the host's exit 143. **Open: no sound.** The TV
  Pulse stream runs on `pcm_output` at 100% but its monitor is all zeros,
  while aka routes game audio to Sunshine's capture. The cause is under
  investigation in the viewer.
- **Gamepad latency** is tracked in #373, the cursor in #370, and the gradient
  noise in Zone's no-art covers in #386 (backlog).
- **Resources (Zone idle).** korrid 12 MB RSS, about 9% CPU; shell 114 to
  130 MB, about 10 to 13% CPU. Zone's title-card loop takes 61 to 140% CPU.
  M6 needs to address this.

Operator scripts and evidence live outside the repository in
`~/.cache/research/korri-webos-m1/` and `~/.cache/research/korri-webos-m2/`.
