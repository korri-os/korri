# Anbernic RG DS

The RG DS runs the shared Korri product session. It now boots from internal
eMMC, not only from SD. Android was erased with the owner's approval on
2026-10-25, without a backup. See [INTERNAL.md](INTERNAL.md) for the install
record, storage layout, label-collision rule, and rescue-SD recovery.

The observations below were recorded on 2026-10-25. Configuration changes in
this tree are not proof that a new image has passed hardware acceptance.

## Hardware and session

| Area | Evidence and configuration |
|---|---|
| Kernel | Shared Linux 7.2, pinned in `../../product/kernel/mainline-7.2.nix`; upstream `rk3568-anbernic-rg-ds.dts`. |
| Bootloader | U-Boot v2026.10-rc4, upstream `anbernic-rg-ds-rk3568_defconfig`; Rockchip BL31/TPL binaries from pinned nixpkgs. These firmware binaries are not fully open firmware. |
| Display | The display-subsystem KMS device has DSI-1 and DSI-2. The current compositor uses only DSI-1 at 640×480, 60 Hz. The portal starts automatically; Chromium still uses `--disable-gpu`. |
| GPU | The running kernel identified Panfrost `fde60000.gpu`, Mali-G52. Linux 7.2 `rk356x-base.dtsi` declares `gpu@fde60000`; Panfrost registers devfreq under the device name. The governor and limits helper now use `fde60000.gpu`. |
| Wi-Fi | RTL8821CS, `rtw88_8821cs`, as on RG353M. At 652 s the RG DS logged `firmware failed to ack driver for entering Deep Power mode`. NetworkManager powersave is now disabled, matching the sister device's link-failure workaround. Stability after this change is unverified. |
| Bluetooth | The RG DS DTS declares `realtek,rtl8821cs-bt` on UART1 with RTS/CTS and wake/enable GPIOs. The shared kernel's generated config lacked `BT_HCIUART_3WIRE` and `BT_HCIUART_RTL`. The device reuses `../rg353m/bluetooth.nix` to select both and enable BlueZ. Controller discovery and pairing remain unverified. |
| DS games | The runtime-installed `@korri:melonds` played Tetris DS. The RG DS image now selects the existing `ds` plugin group, which contains melonDS. This does not select any other DS emulator. |
| Streaming encoder | Before this tree, Sunshine found no encoder on the RG DS (`Unable to find display or encoder during startup`). The image now imports the shared Rockchip RKVENC/MPP support from `../rockchip/` with an `anbernic,rg-ds` overlay. Encoding is unproven on hardware: the modules cannot unload, so the runtime test was stopped. See [the acceptance record](../../../docs/acceptance/rgds-rkmpp-kernel-2026-10-25.md). |
| USB | The DTS makes `usb_host0_xhci` a peripheral. The shared configfs gadget provides serial and USB networking at `10.42.1.1`. A udev rule starts the native getty when `ttyGS0` appears. [USB-CONSOLE.md](USB-CONSOLE.md) records the earlier startup repair. |
| Access | Physical recovery consoles grant root. The image includes the key-only SSH plugin; root authorization remains device-owned. No personal keys or Wi-Fi secrets ship in the image. |
| Builds | Devices consume prebuilt artifacts. Never compile or dispatch builds from the handheld. ARM builds use the configured fuji/yari builders. |

Bluetooth protocol selection changes the RG DS kernel derivation. A new native
kernel build may be needed on fuji/yari if it is not cached. The shared kernel
source and other devices' kernel configurations are unchanged.

System updates are separate from image flashing; see [UPDATE.md](UPDATE.md). The updater has offline tests only and has not run against the device.

## Build and verify without touching hardware

Use the project cache on development machines whose Nix configuration lacks it:

```sh
export NIX_CONFIG="${NIX_CONFIG:-}
extra-substituters = https://korri-os.github.io/nix-cache/
extra-trusted-public-keys = korri-cache-ci-1:iH8gsPMtGrreeuXt2kt6M2ca+y4u/dO+T2km6AUuI8c="

nix build --no-link .#checks.x86_64-linux.rgds
nix build --no-link .#checks.x86_64-linux.rgds-bluetooth
nix build --no-link .#checks.x86_64-linux.rgds-rescue
nix build --no-link .#checks.x86_64-linux.korri-product-image-plugins
nix build --no-link .#checks.x86_64-linux.korri-product-module

# Native ARM image assembly on the configured builders, never on the device:
nix build --no-link .#packages.aarch64-linux.rgds-sd-image
nix build --no-link .#packages.aarch64-linux.rgds-rescue-sd-image
```

`nixosConfigurations.rgds-rescue` retains the full product composition but uses
`KORRI_RESCUE` for root and `KORRI_RBOOT` for the firmware partition. Use that
image for a rescue SD when eMMC already holds the normal system. Do not insert
the old SD with `NIXOS_BOOT` / `NIXOS_RGDS`: it can mount the wrong root.

The distribution workflow builds `packages.aarch64-linux.rgds-sd-image` and
records its source revision and checksum. See
[IMAGE-BUILDS.md](../../formats/IMAGE-BUILDS.md). Verify a downloaded
distribution before decompression:

```sh
nix run .#device-image-verify -- /path/to/download FULL_COMMIT_SHA
```

Every image build runs `verify-image.py` before compression. It checks the MBR,
filesystem labels, embedded U-Boot bytes, and kernel/initrd/board-DTB references
in the selected extlinux entry. Repeat the structural check on a regular image
file using the exact U-Boot binary from its native ARM build:

```sh
nix run .#rgds-image-check -- /path/to/image.img /path/to/u-boot-rockchip.bin
# For a rescue image:
nix run .#rgds-image-check -- /path/to/rescue.img /path/to/u-boot-rockchip.bin \
  --root-label KORRI_RESCUE --firmware-label KORRI_RBOOT
```

The verifier never mounts filesystems or opens block devices. It copies the
root partition to temporary storage, so allow space for that copy. Its offline
tests use real temporary filesystems. It does not prove hardware boot or full
filesystem integrity. `rgds-initrd-check` exercises the real NixOS module
shrinker; on x86_64 it cross-builds the kernel and can be expensive on a cold
store. `nixos-layout-check` is the broader shared layout task.

## Open hardware questions — no product decision made

| Area | Recorded limit or question |
|---|---|
| Lid | The hall sensor appears as `gpio-keys-hall`. With no verified sleep state, logind has `HandleLidSwitch=poweroff`. Lid behavior still needs acceptance. |
| Power key | A short press powers off; logind has `HandlePowerKey=poweroff`. No suspend or light-sleep path has been verified. See [SLEEP.md](../../product/SLEEP.md). |
| Touch | There are two Goodix controllers. Touch-to-panel mapping is unresolved. |
| Thermal | A 70 °C passive trip and a 1608 MHz cap were reached about 12 minutes into melonDS play. Thermal policy is unresolved; this change adds no CPU cap. |
| Two screens | Only DSI-1 is used by the current session. Two-screen presentation is unresolved. |

After a separately approved deployment, verify Bluetooth discovery and pairing,
Wi-Fi stability without deep power saving, GPU devfreq control, and a real
melonDS launch. Rescue-SD boot and its root/firmware mounts also need hardware
acceptance. Do not infer those results from evaluation checks.
