# RG353M persistent deployment and streaming audio — 2026-09-06

## Scope

Persist the tested mainline/RKVENC/zero-copy system on the existing SD card and enable streaming audio. No eMMC writes, kernel-family changes, pairing-state resets, or federation changes.

## Implementation

- Shared host opt-in: `services.korriLinuxHost.audio.enable` (off by default).
- Standard NixOS per-user PipeWire, Pulse compatibility, WirePlumber and RTKit; gameplay linger and audio-device group membership support the headless user.
- Sunshine uses the gameplay user's explicit local Pulse socket and session bus. Existing capture/input/compositor sandbox policy remains intact; no anonymous Pulse authentication or network audio listener was added.
- PipeWire services start at the user manager's default target. A bounded, unprivileged Sunshine pre-start check waits for the Pulse socket and WirePlumber's `default` metadata. The existing privileged compositor-readiness check is unchanged.
- RG353M opts in and sets NetworkManager's default Wi-Fi powersave policy off. Its existing workshop profile also specifies powersave off. `schedutil` remains selected.

## Reproductions and verification

1. Original streams logged `Couldn't connect to pulseaudio: Access denied`; no audio daemon or gameplay Pulse socket existed.
2. The initial PipeWire configuration worked after diagnostics started the server, but the first post-reboot stream exposed a lazy-start race: `Couldn't set default-sink ... Not supported`. Sunshine reached the Pulse endpoint before WirePlumber created default metadata.
3. Eager startup plus the readiness gate fixed the first-session race. The final post-reboot test made no preliminary PipeWire client connection before Moonlight.
4. Final boot: `/run/booted-system`, `/run/current-system` and persistent system profile select:

   `/nix/store/dim0xbldg2dvd5j6lib88hrx1zbmh98c-nixos-system-rg353m-sd-card-26.05.20251221.a653104`

5. Mainline Linux 6.18.2 loaded `rk_vcodec` and probed RKVENC from the production boot device tree; `/dev/mpp_service` was root:video 0660. The temporary runtime overlay module was not required.
6. Sunshine used package `9xn6z4h9gk7gbcs9s9gd7wkniz74jlsv`; first final-boot stream logged `Using zero-copy KMS DRM_PRIME capture for RKMPP` and `Opus initialized: 48 kHz, 2 channels, 96 kbps (total), LOWDELAY`.
7. Moonlight Embedded fake/view-only client requested 640x480@60, H.264, 8000 kbps. Its final first-session log contained `Received first audio packet after 700 ms`, with no `No audio traffic` warning; the test explicitly checked both conditions rather than only the process exit. Sunshine's graph linked both capture channels to the virtual sink monitor.
8. A synthetic 440 Hz left / 660 Hz right tone was played to the Sunshine sink. A separate sink-monitor recording (not microphone capture) contained 48 kHz stereo samples, peak 2500 and RMS approximately 1030 on both channels. This confirms the local render/monitor path, not decoded client fidelity. Physical audible left/right confirmation remains a user check.
9. Wi-Fi joined at 192.168.1.141 and reported powersave off without a manual `iw` command. Ethernet remains 192.168.1.239.
10. aarch64 and x86 host-module checks passed, including opt-in audio, default-off behavior, linger, user-service startup, local Pulse endpoint and readiness gate. x86 Sunshine derivation remains `/nix/store/sxgpqashwhgj6gfmaha29iszzl74nmpd-sunshine-korri-2025.924.154138-korri.drv`.

## Persistence and rollback

- Verified `/` and `/boot` resolve to SD partition `/dev/mmcblk1p2` before updating the system profile and extlinux files.
- Used normal NixOS generation/profile operations. No raw disk, U-Boot, eMMC, or runtime-overlay removal command was used.
- Final boot installation used `switch-to-configuration boot`, then a normal reboot. This is reboot verification, not a power-removal/cold-start test.
- Original extlinux config and system-profile path are saved on the device under `/root/rg353m-persistent-backup-20260906/`.
- Previous generations remain in `/boot/extlinux/extlinux.conf`, including original `nixos-1-default`.
- To roll back over SSH, read the original path from `system-profile.before`, set `/nix/var/nix/profiles/system` to it with `nix-env --profile ... --set`, run that closure's `bin/switch-to-configuration boot`, then reboot. Verify `/boot` is still on the SD card first. Do not unload the runtime MPP overlay.

## Remaining observations outside this scope

The existing `systemd-networkd-wait-online` timeout delays Sunshine startup by roughly two minutes. A switch of the already-running initial generation also reported the pre-existing USB gadget symlink collision; fresh reboot initialized the gadget. Neither issue was silently counted as a successful switch. The final boot-only install succeeded and the selected configuration was verified after reboot.

The deployed Sunshine includes the earlier stdin-PIN return fix; pairing cleanup/review was not part of this audio/persistence slice. Fake-client transport evidence does not replace physical listening or constitute a new performance benchmark.

Local evidence: `/tmp/rg353m-audio-ready-build.log`, `/tmp/rg353m-audio-ready-x86.log`, `/tmp/rg353m-audio-final-boot.log`, `/tmp/rg353m-audio-final-first-stream.log`.
