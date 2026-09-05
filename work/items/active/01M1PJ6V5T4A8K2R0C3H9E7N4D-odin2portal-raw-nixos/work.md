---
id: 01M1PJ6V5T4A8K2R0C3H9E7N4D
title: Raw NixOS on the AYN Odin 2 Portal
status: active
created: 2026-09-04
source: direct
---

# Raw NixOS on the AYN Odin 2 Portal

Run NixOS directly on the Portal with no ROCKNIX host and no nspawn guest,
the same shape as the RG353M in `nix/rg353m/`. The device already runs
ROCKNIX-ABL (a closed binary in `abl_a`/`abl_b` that loads an Android boot
image named `/KERNEL` from a FAT partition on UFS, SD, or USB), so the
bootloader is done and must never be touched again. Everything Korri
produces is a file ABL loads plus an ext4 root.

## Hard rule

Never require the case to be opened. See the AGENTS.md standing decision:
no writes to `abl_*`, `xbl_*`, or any firmware partition, ever. First boots
go through tethered `fastboot boot` or an SD-card root. Internal
`sda18`/`sda19` writes need explicit approval per run.

## Slices

1. **Kernel** — done (`ee4af7f3`). Linux 7.0.2, 5 mainline + 48 SM8550
   patches, AYN DTS, ROCKNIX defconfig. Cross-built from x86_64 because
   fuji cannot spare the ~30 GB a kernel compile needs.
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
6. **Userspace** — not started. Sway, gamescope, inputplumber, korrid from
   `legacy:product/systems/nixos/images/platforms/rocknix-sm8550.nix`.

## Verified on device

| | |
|---|---|
| Boot | 35 s, U-Boot -> systemd-boot -> NixOS |
| GPU | Turnip Adreno 740, GL 4.6, glmark2 1624 |
| WiFi | `vrackie` autoconnect, 1080 Mbit/s TX |
| Audio | UCM HiFi: Speaker/Headphones/DisplayPort; 440 Hz tone audible |
| Thermals | fan 0 RPM at 32 C under the whisper curve |
| GMU guard | `cpu0/cpuidle/state1` held disabled |
| Android | untouched; BOOT MODE = Android returns to it |

## Open questions

- **Userspace session owner.** PipeWire carries `ConditionUser=!root` and
  this image logs in as root, so no sink exists yet. Sinks appear when a
  real session user owns the graph (legacy: the Korri runtime user under
  greetd). Blocks slice 6, not the substrate.
- **Fake suspend.** S3 does not work on this SoC. sleep/suspend/hibernate
  are disabled and logind ignores the power key so a failed resume cannot
  masquerade as a hang. The real behaviour is a product concern owned by
  the session and input layers.
- **SD bus mode.** The card negotiates plain high speed (~25 MB/s), not
  SDR104. armbian carries
  `0217-arm64-dts-Switch-to-downstream-sdhc-driver-for-Odin2.patch` for
  exactly this. Measure before applying.
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

- ROCKNIX rev `f080b462f54b5807bdd16ac7cc2ab64528b038b1` (next, 2026-05-13),
  from `~/code/sandbox/nix-on-rocks/upstream.lock`.
- Safety audit: `nix-on-rocks/docs/ops/sm8550-full-install-safety-audit-2026-05-20.md`
  (UFS is 4096-byte sectors; `sda18` = ROCKNIX FAT, `sda19` = STORAGE ext4).
