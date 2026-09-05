---
id: 01M1PJ6V5T4A8K2R0C3H9E7N4D
title: Raw NixOS on the AYN Odin 2 Portal
status: active
created: 2026-09-04
source: direct
---

# Raw NixOS on the AYN Odin 2 Portal

Run NixOS directly on the Portal with no ROCKNIX host and no nspawn guest,
the same shape as the RG353M in `nix/rg353m/`. AYN U-Boot 2025.01 in
`loader_a` starts the UEFI removable-media fallback from the SD-card ESP.
The card carries systemd-boot, an EFI-stub kernel, the initrd, the Portal
DTB, and an ext4 root. Nothing Korri produces changes internal storage.

## Hard rule

Never require the case to be opened. See the AGENTS.md standing decision:
no writes to `abl_*`, `xbl_*`, or any firmware partition, ever. First boots
go through tethered `fastboot boot` or an SD-card root. Internal
`sda18`/`sda19` writes need explicit approval per run.

## Slices

1. **Kernel** — done (`ee4af7f3`, Linux 7.2 update pending commit). Linux
   7.2 with 4 mainline + 2 version + 52 SM8550 patches, AYN DTS, and the
   ROCKNIX kernel configuration. The version-specific DPU cleanup patch is
   disabled because it stops the Portal during early boot. Every generation
   includes a Linux 7.0.2 rescue specialisation. Cross-built from x86_64
   because fuji cannot spare the ~30 GB a kernel compile needs.
2. **Firmware** — done (`897af5a1`, extended in `6b96bafd`). 15 AYN blobs
   plus the three Adreno 740 files, all pinned by hash, none in git. Two are
   corrections rather than additions: ath12k `board-2.bin` and
   `vpu30_p4.mbn` differ from the nixpkgs copies and ROCKNIX's win.
3. **Boot** — done (`3f6c9c77`, `6b96bafd`). Not mkbootimg: AYN ships U-Boot
   2025.01 in `loader_a`, reached by BOOT MODE = Loader, and it runs
   `bootefi bootmgr`. So the card is a GPT disk with an ESP carrying
   systemd-boot. Nothing on internal storage is written.
4. **First boot** — done. Root shell on the panel, then SSH over WiFi.
5. **Platform policy** — done (`08fe8858`). Governors, GMU guard, fan curve,
   kernel cmdline, audio UCM. Ported from ROCKNIX/legacy, each verified on
   hardware.
6. **Userspace** — runtime identity bootstrap done. `korri:korri` uses UID/GID
   `1000`, home `/home/korri`, and a lingering user manager. PipeWire and
   WirePlumber run without greetd. Compositor, input, and game services have
   not started.

## Verified on device

| | |
|---|---|
| Boot | Linux 7.2.0, AYN U-Boot -> systemd-boot -> NixOS; Linux 7.0.2 rescue specialisation retained |
| Kernel patches | 58 ROCKNIX patches plus the nixpkgs RANDSTRUCT seed; `0010-msm-resource-cleanup.patch` disabled after a controlled hardware test |
| GPU | Turnip Adreno 740, GL 4.6; Linux 7.2 GPU load produced no GMU timeout |
| WiFi | `vrackie` autoconnect, 1080 Mbit/s TX |
| Audio | `korri` PipeWire graph; UCM HiFi Speaker/Headphones sinks; 440 Hz tone audible |
| Input | AYN gamepad, touch screen, and two force-feedback devices present; both haptic writes felt |
| SD bus | 37.5 MHz, 4-bit SD high-speed at 3.3 V; UHS is not active |
| Thermals | fan 0 RPM at 32 C under the whisper curve |
| GMU guard | `cpu0/cpuidle/state1` held disabled |
| Android | untouched; BOOT MODE = Android returns to it |

## Open questions

- **Fake suspend.** S3 does not work on this SoC. sleep/suspend/hibernate
  are disabled and logind ignores the power key so a failed resume cannot
  masquerade as a hang. The real behaviour is a product concern owned by
  the session and input layers.
- **SD bus mode.** The card negotiates 37.5 MHz, 4-bit SD high-speed at
  3.3 V. UHS is not active. armbian carries
  `0217-arm64-dts-Switch-to-downstream-sdhc-driver-for-Odin2.patch` for
  this case. Port the pinctrl and clock wiring in a separate slice.
- **`firewall.service` fails.** The ROCKNIX config likely lacks the
  netfilter modules NixOS wants; legacy hit the same thing and ran
  Tailscale with `--netfilter-mode=off`. Owner deferred it.
- **First switch on a fresh card** needs `--install-bootloader`: U-Boot's
  UEFI keeps no EFI variables, so `bootctl status` exits non-zero and the
  builder reads that as no bootloader present. Later switches are
  unattended.
- The AYN DTS series (`ayntec,odin2portal`, v8, 2026-05-03) is still not in
  torvalds master, so the patch queue is re-synced by hand on kernel bumps.

## Evidence

- ROCKNIX rev `1178bc2238de782bf081c558c177d35bb3690021` (next, 2026-09-04).
- Controlled early-boot isolation: Linux 7.2 failed with both the new and
  Linux 7.0.2 DTBs when `0010-msm-resource-cleanup.patch` was active. The
  same kernel started with either DTB after that patch was disabled.
- Safety audit: `nix-on-rocks/docs/ops/sm8550-full-install-safety-audit-2026-05-20.md`
  (UFS is 4096-byte sectors; `sda18` = ROCKNIX FAT, `sda19` = STORAGE ext4).
