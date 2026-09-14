# AISLPC R36T Max

Standalone NixOS on SD. The device sources now live here. Stock-firmware SSH
reconnaissance remains under `nix/spikes/rk3326-stock-shell`; no device
configuration imports that spike.

## Acceptance state

The previous kernel displayed a NixOS root prompt with `rocknix,generic-dsi`.
The current full images are candidates until tested on the handheld. Do not
substitute a successful build for a picture, working radio, or memory result.

| Package under `packages.aarch64-linux` | Purpose | SSH |
|---|---|---|
| `r36tmax-recovery-sd-image` | Console baseline with the preserved ROCKNIX loader. | Disabled. |
| `r36tmax-sd-image` | Existing Korri stack with that same loader. | Disabled. |
| `r36tmax-diagnostic-sd-image` | Owner-operated Korri image for remote diagnosis. | Key-only; contains no authorized key. |
| `r36tmax-mainline-sd-image` | Console boot test with mainline U-Boot and Rockchip DDR code. | Disabled. |

All images retain `NIXOS_BOOT` and `NIXOS_R36TMAX`, a 16 MiB loader area,
128 MiB FAT, and LZO initrd compression. eMMC remains disabled. No install
step writes internal storage. The first three images build their FAT files,
root and modules from one system derivation. They need no `KORRI_KERNEL` step.
The image verifier checks the actual file image before compression.

The mainline loader remains a separate candidate. Its exact DDR, SPL, FIT and
combined-image checks pass, but cold starts, warm restarts and recovery are
not verified. Read `bootloader/README.md` before selecting it.

## Build and inspect

Use a build machine, never the handheld. `fuji` is the established ARM builder.

```sh
nix run .#r36tmax-check
nix build .#checks.x86_64-linux.r36tmax-recovery
nix build .#packages.aarch64-linux.r36tmax-recovery-sd-image
nix build .#packages.aarch64-linux.r36tmax-sd-image
```

`nix run .#device-image-dist` stages a pinned image, checksum and revision from
a clean checkout. Its image argument is
`packages.aarch64-linux.r36tmax-sd-image`. The separate signed-cache workflow
is configured to publish the public system closure and every kernel output.
Publication has not run for this candidate. The diagnostic image is not in
its device list. Preserve signature checking and the no-build policy.

Normal boot-generation installation is explicitly blocked. The preserved
loader reads FAT, while the generic NixOS installer targets root `/boot`.
Until a transactional FAT installer passes rollback tests, write a complete
SD image for boot updates. Downloading a closure does not select it at reboot.

The recovery loader and RK915 firmware have separate distribution obligations.
Read `recovery/README.md` and `wifi/README.md`. Image artifact/release CI is
blocked until the loader notices and corresponding sources are supplied.
Do not publish firmware, private provisioning files, diagnostic images, or
the full ROCKNIX input archive.

## Owner provisioning and physical tests

Public images contain the RK915 module, not its two firmware blobs. The native
Linux firmware loader also searches `/lib/firmware`, after its configured
firmware path. Before the first radio test, put the owner's exact
`rk915_fw.bin` and `rk915_patch.bin` there on the SD root. `wifi/README.md`
records their hashes. No calibration file is copied.

Use the existing NetworkManager contract `/etc/korri/wifi.env` for `WIFI_SSID`
and `WIFI_PSK`. Provision it after the public image build, with mode 0600.
For the diagnostic image, put the owner's public key in
`/root/.ssh/authorized_keys`, with directory mode 0700 and file mode 0600.
Never copy the private key. These are existing Linux/OpenSSH and `nix/base`
contracts, not new Korri data formats.

Before any write, verify the live reader's model, serial, capacity and mounted
partitions. Never reuse a remembered `/dev/sdX`. Keep a copy of the working
card's boot files, loader and logs. `write-image` rejects mounted/oversized
media and malformed or oversized compressed images, then reads back all
written image bytes. It does not authorize a disk by itself.

Insert the SD card only while the handheld is off. Power it on normally.
Once SSH works, shut down with `systemctl poweroff`, wait for shutdown, then
remove power and the card. Without SSH or a verified power-button shutdown
path, this host cannot confirm graceful shutdown. Never remove a powered card.

After console and candidate boots, run:

```sh
nix run .#r36tmax-session-check -- root@r36tmax
```

This observes services, memory, pressure, OOM counters and the static origin.
It does not prove pixels, authenticated RPC, interaction, stability, or 1 GB
fit. Capture those separately. The helper takes at most 47 seconds.

## Hardware facts and unresolved work

- The retained display description uses 720 by 720 pixels, 50 MHz and 1080 by
  764 totals. The saved console record reports DSI-1 and 61 Hz. Sway selection
  of this approximately 60.60 Hz mode remains a physical test.
- `platform-display-subsystem-card` follows the observed DRM device. The
  expected `platform-ff400000.gpu-render` link follows the enabled platform
  node and udev convention, but is not present in the saved survey. The
  candidate loads Panfrost and fails normally if its render node is absent.
- The existing full Linux host starts Sunshine, InputPlumber, inputd,
  Sway/Xwayland and korrid. The private kiosk adds Chromium and
  static-web-server. This is not a minimal daemon-only memory test. It adds no
  controller profile, local compositor input, audio or rumble implementation.
- The loopback test relay is the existing Odin/RG353M local-session precedent.
  It is not federation discovery. No owner or peer binding is invented.
- `wifi/mmc-support.patch` comes from the pinned RK915 proof, with one safety
  correction: malformed CIS tuples shorter than the two-byte block-size
  read are rejected. The radio-specific properties do not apply to SD/eMMC.
  No SDIO supply voltage changes are made. Existing supply discrepancies
  remain a hardware question.
- `dts/PANEL-PMIC-AUDIT.md` records the exact source comparison. All 22 vendor
  register writes match. Generic and ST7703 still differ in timing, reset,
  supply binding and error handling. Generic remains selected. RK809 switch
  descriptors are not missing RK817 descriptors. Backlight feed and actual
  PMIC silicon identity remain unmeasured.

Known-good local artifacts and raw logs remain under the existing
`~/.local/share/korri/rk3326-stock-shell/` directory. Moving source files does
not rename that user-owned evidence.
