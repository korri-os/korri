# RG353M GBA launch acceptance — 2026-09-07

## Scope and preservation

This slice uses the existing hostless `app.session.prepare` executor, not a
second Linux spawn bridge. Linux portal authority is limited to existing reads,
hostless prepare and exact `expectedLaunchId` stop. Peer prepares and unrelated
mutations remain denied.

The device is tested through a composed runtime in
`/tmp/rg353m-gba-launch-runtime` on the coordinator and native aarch64 builder
`fuji`. It overlays this branch onto the preserved streaming runtime; plain
main is **not** an equivalent deployment. The stage imports
`nix/rg353m/gba-gameplay.nix` from its existing portal preview composition.

Kernel, modules, DT, compositor unit and Sunshine unit were compared with the
previous runtime. Controller behavior executables and profiles are pinned to
the previous release; only the validated-target ACL helper changes. x86
Sunshine remains exactly:

```
/nix/store/sxgpqashwhgj6gfmaha29iszzl74nmpd-sunshine-korri-2025.924.154138-korri.drv
```

Original ROMs and eMMC are not modified. `/boot` remains on `/dev/mmcblk1p2`.
Before import, catalog/private state and original closures were backed up in
`/root/korri-gba-launch-20260907` (private, not committed).

## Catalog

The offline `korrid catalog import` command ran as korrid with the daemon and
control socket stopped, against its explicit existing public/private roots.
It invokes production discovery; no library records were handwritten.

- Seven original files retain their verified SHA-256 values.
- The archive remains intact. Its single GBA member was separately extracted
  in place and verified: SHA-256
  `eef5120a58feaf194d3b47c67ba30e249217953e898cdcf5c3297f1c531ba24f`.
- Discovery found seven GBA candidates, added six unique game records and
  removed no records/releases. It reported the unclaimed ZIP and duplicate
  Wario content; both originals remain.
- Authenticated catalog RPC returns six local, hash-identified games. Pico
  renders those games, not fixtures.
- The independent minimum-config worktree is not integrated into this
  deployment. Registration uses this runtime's existing production producer;
  rerun producer/resolver tests before adopting a new document format.

## Runtime fixes and observed behavior

### Session lifecycle and presentation

Real Linux status omits `host`; completion is `Err(SessionCompleted)` and idle
is `Err(NoActiveSession)`. Regression tests use these shapes. They cover
completion, exact stop, catalog failure, stale prepares, active conflicts and
same-id peer copies. Observation errors do not authorize duplicate launch.

Pico observes an acknowledged resumable session disappearing and returns to
the library. Unavailable catalogs, failed starts and replacement sessions do
not trigger that transition. Confirm recovers native control focus after page
unmount, including modal and attract-mode restrictions.

Chromium app startup is shell-owned, using one inert data-URL page. The private
CDP pipe installs the bridge before trusted navigation. Caller app/debugging
flags remain forbidden. The failed `--app=about:blank` experiment was discarded:
real Chromium created a new tab. Headful tests cover the replacement app mode.

### Access and audio

Gameplay receives ROM reads and explicit writable save/state directories,
not configuration reads. Existing top-level YAML files receive named deny
ACLs; default deny ACLs protect newly created descendants. Defaults do not
retroactively protect arbitrary existing trees. Explicit account paths
replace the inherited deny with their required gameplay access. A tmpfiles
`z` rule prevents the base directory's `0700` creation rule from masking the
named traversal ACL on subsequent activation.

Owned game units expose only the Pulse directory at `/run/korri-game-audio`
when the trusted host explicitly selects that socket. The rest of the user
runtime, compositor control, private daemon/Sunshine state and raw claimed
input sources remain inaccessible.

RetroArch's upstream udev driver required `O_RDWR` and silently skipped the
read-only virtual gamepad. The reviewed patch retries only permission failures
with `O_RDONLY` and disables force-feedback writes on such descriptors. It
never grants gameplay event-injection permission. The dedicated portal user
is added to the existing exact-target ACL validator, not the input group.

Actual Donkey Kong launch opened the normalized target read-only. Injected
GPIO-source Start/Confirm traversed InputPlumber and moved the game into its
save-slot menu. These are real input-path tests, not a human physical-button
or tactile-rumble acceptance claim.

The previous default audio output was HDMI. The existing gameplay
WirePlumber default was changed to the observed speaker sink at 20% volume.
The game created a running Pulse stream. A speaker-sink monitor recording
contained 165,888 stereo frames at 48 kHz, signed 16-bit; both channels had
RMS approximately 2037 and peak 10643. This proves nonzero game output in the
speaker graph, not human-audible or Moonlight-decoded acceptance.

### Thermal constraint

At the previous 1.8 GHz ceiling, game tests reached the 68°C cutoff. An early
thermal harness also lacked `sleep` on PATH and was corrected; its first
measurement is not clean performance evidence. The corrected guard separately
observed a 68.125°C cutoff during Donkey Kong startup.

With schedutil and a 1.104 GHz ceiling, Wario ran over 20 minutes around 57°C;
Donkey Kong with controller input was around 56°C. The opt-in board profile
uses the existing NixOS `powerManagement.cpufreq.max` option. It does not
change kernel drivers or enable performance/ondemand governors. These samples
are not a full thermal-envelope or decoded-frame-rate benchmark.

### Verified before final controller-loop candidate

- Pico Play started an owned real RetroArch/mGBA service.
- Physical-source injected inputs reached the game via its read-only target.
- Exact-session stop returned the same borderless Chromium app to Pico's
  library at 640×480, without tabs/address chrome.
- Save and automatic-state files were created as gameplay in the intended
  writable account directories.
- ROM manifest verification still passes after launch/stop.

## Automated checks

- Final portal suite: 333 tests and typecheck passed.
- Pico suite: 338 tests and typecheck passed.
- Linux shell: 21 tests including real headful Chromium and origin/readiness
  security cases passed independently in review.
- RetroArch extracted-C regression passes and rejects five unsafe mutations;
  actual patched RetroArch built natively on fuji.
- Targeted Rust launcher, Pulse-isolation and natural-completion tests pass.
- Native aarch64 portal/input Nix module checks pass.
- Independent reviews found no remaining blockers in the launch, narrow
  authorization, read-only RetroArch, app-window and focus changes.

## Controller-only loop, persistence and reboot

Before persisting, the loop was exercised twice with physical GPIO-source
input only: Confirm opened the game screen, Confirm started an owned
RetroArch unit, and **Select+Start** quit the emulator. Pico returned to its
library each time and stayed usable. After a return, the first directional
press seeds focus on the first control rather than moving; that is the
recovery behavior, not a launch of an unintended game.

The tested closure was then installed as the boot configuration and the
device was rebooted normally. Post-boot observations:

- Booted system equals the installed generation; previous generations remain
  selectable, and generation 9 is the earlier streaming runtime.
- `korrid`, kiosk, Sunshine, compositor, `korri-inputd` and InputPlumber are
  active. The bundle is the pinned controller release with only korrid replaced.
- CPU governor remains schedutil with the 1.104 GHz ceiling.
- The normalized target moved to `/dev/input/event2`. Its ACL again grants
  read to `korri-inputd`, `korri-portal` and the gameplay identity only.
- ROM reads succeed, catalog documents stay denied, saves stay writable.
- Controller-only launch reached the Donkey Kong title screen. A speaker-sink
  monitor recording captured 158,720 stereo frames, RMS about 1870, peak 12244.
- Select+Start exited to `Err(SessionCompleted)` and Pico showed the library.
- Streaming regression: Sunshine selected `h264_rkmpp` with zero-copy KMS
  DRM_PRIME capture, Opus 48 kHz stereo; the client received the first video
  packet at 0 ms and first audio at 900 ms. ROM manifest still verifies.

Temperatures during these runs stayed between roughly 45 and 57°C.

## Repository wiring note

The device ran a composed stage that overlays this branch onto the preserved
streaming runtime, whose host module exposes `gameplayUser`/`gameplayGroup`.
The committed module uses this branch's existing `runtimeUser`/`runtimeGroup`
options, which resolve to the same gameplay identity and group. The committed
`rg353m-portal-preview` configuration evaluates with the access module wired.
Rebuild and re-verify when the preserved runtime's modules land on main.

## Not covered

A normal reboot is not a power-removal cold boot. ADC stick normalization,
kernel-level gameplay input isolation beyond the existing controls, decoded
frame-rate benchmarking, human-audible confirmation, and multi-hour thermal
soak remain outside this acceptance. The independent minimum-config document
format is not integrated here.
