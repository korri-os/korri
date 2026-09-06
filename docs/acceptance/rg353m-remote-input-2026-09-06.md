# RG353M Moonlight desktop input — 2026-09-06

## Reproduction

Moonlight touchscreen gestures did not reach the Wayland validation page. Sunshine had created relative mouse, absolute mouse and keyboard uinput devices (`beef:dead`, version `0111`), but Sway `get_inputs` returned `[]`. The shared physical compositor explicitly selected `WLR_BACKENDS=drm`, which omits libinput.

## Fix

Add default-off `services.korriLinuxHost.compositor.remoteInput.enable`, restricted to the DRM backend, and opt the RG353M in. This selects `drm,libinput`. The generated Sway configuration disables input events globally, then enables only Sunshine's five named vendor/product-qualified virtual devices:

- `48879:57005:Mouse_passthrough`
- `48879:57005:Mouse_passthrough_(absolute)`
- `48879:57005:Keyboard_passthrough`
- `48879:57005:Touch_passthrough`
- `48879:57005:Pen_passthrough`

No `input` group membership, raw-device ACL, uinput grant, or source-hiding policy was broadened. Physical devices may be enumerated/opened by the trusted compositor's existing seatd backend, but their event delivery is disabled. This is compositor event filtering, not a new kernel isolation boundary. Controller routing through InputPlumber is unchanged. Other hosts retain the old default behavior.

## Verification

- Native aarch64 full closure and host-module check passed.
- x86 host-module check passed. Tests cover DRM-only opt-in, unchanged default, no broad input group, and the generated default-deny/allowlist rules.
- x86 Sunshine derivation remains `/nix/store/sxgpqashwhgj6gfmaha29iszzl74nmpd-sunshine-korri-2025.924.154138-korri.drv`.
- Runtime Sway inspection showed Sunshine mouse/absolute mouse/keyboard enabled and physical keys, HDMI inputs, and Hynitron touchscreen disabled.
- Injected a synthetic absolute move and left-button click through the existing Sunshine evdev devices, checking virtual sysfs ancestry and exact vendor/product/name before writing. Chromium received the click and changed its title to `RG353M input verified: 1 clicks - Chromium`. This verifies the virtual-device-to-Wayland application path; it does not substitute for a physical Moonlight touch test.

## Deployment

Installed SD boot generation:

`/nix/store/zs8x0bagc75hkwbfbaxlq23k4fcvs9h5-nixos-system-rg353m-sd-card-26.05.20251221.a653104`

Used `switch-to-configuration boot` after verifying `/boot` resides on `/dev/mmcblk1p2`. A normal reboot removes the temporary test override; the generated configuration contains the backend and allowlist. eMMC was not written. Audio configuration and the static RKVENC overlay remain included.

The three-entry extlinux limit now retains the two preceding audio generations rather than the original pre-audio entry. The original system-profile backup still identifies its retained Nix generation; reinstall its boot files with `switch-to-configuration boot` before attempting that older rollback. Do not blindly restore an extlinux file referencing boot files already pruned by the generation limit.

Post-reboot verification confirmed the selected boot closure, no compositor runtime drop-ins, `drm,libinput`, and powersave off. The event allowlist passed for all eight enumerated input entries, and the synthetic click again reached Chromium. A subsequent 640x480@60 fake-client regression stream logged zero-copy KMS capture and stereo Opus initialization; its first audio packet arrived after 800 ms.

Physical Moonlight touchscreen confirmation is pending. In Moonlight, trackpad mode clicks at the cursor; direct-touch mode maps taps to the tapped coordinates.

Evidence: `/tmp/rg353m-remote-input-build.log`, `/tmp/rg353m-remote-input-x86.log`, `/tmp/rg353m-remote-input-boot.log`.
