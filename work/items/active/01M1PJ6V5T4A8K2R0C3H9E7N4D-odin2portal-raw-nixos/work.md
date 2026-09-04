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

1. Kernel package (this slice). Rebuild the ROCKNIX SM8550 kernel under Nix
   from the rev nix-on-rocks pins and sobo runs: Linux 7.0.2, 5 mainline +
   48 SM8550 patches, AYN DTS, SM8550 defconfig. Output: `Image`, modules,
   `qcs8550-ayn-odin2portal.dtb`. Exposed as
   `packages.{aarch64,x86_64}-linux.odin2portal-kernel` (native and cross).
2. Firmware package. 129 files / 121 MB from
   `projects/ROCKNIX/devices/SM8550/filesystem/.../lib/firmware`
   (adsp, cdsp, a740_zap, ath12k WCN7850, VPU, aw883xx). Proprietary; a
   local derivation kept out of git, same posture as the stock Android
   images in `clients/android/firmware/odin2portal/`.
3. Boot image. `mkbootimg` (nixpkgs `android-tools`) header v0, gzip
   `Image` + DTB concatenated, NixOS initrd, cmdline `root=LABEL=...`.
   Copies ROCKNIX `makeinstall_target` in `packages/linux/package.mk`.
4. Tethered first boot: `fastboot boot` with root on SD. Target: SSH over
   WiFi, the same bar as the RG353M first slice.
5. Port the guest userspace from
   `legacy:product/systems/nixos/images/platforms/rocknix-sm8550.nix`
   (Sway, gamescope, PipeWire + AYN UCM, inputplumber, fan curve, clock
   governor, fake suspend, seat ACLs). Drop the nix-on-rocks device-facts
   layer and the `nsenter` deploy.

## Open questions

- Generations. ABL boots one `/KERNEL`; there is no NixOS boot menu.
  Rollback of kernel/initrd means swapping the file. Userspace rollback
  through `nixos-rebuild --rollback` still works. Decide between a
  `/KERNEL.prev` convention and treating the SD/USB source as the recovery
  lane.
- Whether stock ABL can boot from SD at all is irrelevant while sobo runs
  ROCKNIX-ABL, but matters for a second device.
- The AYN DTS series (`ayntec,odin2portal`, v8, 2026-05-03) is not in
  torvalds master yet. Until it lands the patch queue is re-synced by hand
  on every kernel bump.

## Evidence

- ROCKNIX rev `f080b462f54b5807bdd16ac7cc2ab64528b038b1` (next, 2026-05-13),
  from `~/code/sandbox/nix-on-rocks/upstream.lock`.
- Safety audit: `nix-on-rocks/docs/ops/sm8550-full-install-safety-audit-2026-05-20.md`
  (UFS is 4096-byte sectors; `sda18` = ROCKNIX FAT, `sda19` = STORAGE ext4).
