# webOS (LG B7) streaming-only Korri: milestones (2026-11-06)

Goal: a streaming-only Korri on the LG B7 (webOS 3.9.3) with the full native
UI. Each milestone ends in something the owner can check on the TV.

| # | Milestone | Owner's check on the TV | Status |
|---|---|---|---|
| M0 | TV healthy | The old spike app reaches the front; D-pad and OK work | Done 2026-11-06 |
| M1 | Real shell shows Zone | Zone at 1080p; D-pad, OK and Back work | Done 2026-11-06 |
| M2 | korrid on the TV, joined to the owner | Zone lists the real games from `haku` | Done 2026-11-06, landed `2d6b2b3b7..649506ea8` |
| M3 | Survives real life | App restart, reinstall and TV reboot keep identity and library | Restart and reinstall pass; reboot needs the permission fix below |
| M4 | First stream | A haku game in Zone plays on the TV; quitting returns to Zone | In progress: an embedded viewer inside `org.korri.shell` (owner decision, 2026-11-07) |
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
- **Resources (Zone idle).** korrid 12 MB RSS, about 9% CPU; shell 114 to
  130 MB, about 10 to 13% CPU. Zone's title-card loop takes 61 to 140% CPU.
  M6 needs to address this.

Operator scripts and evidence live outside the repository in
`~/.cache/research/korri-webos-m1/` and `~/.cache/research/korri-webos-m2/`.
