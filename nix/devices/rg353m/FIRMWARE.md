# RG353M firmware reduction

Phase 1 removes only `amdgpu/`, `radeon/`, `nvidia/`, `i915/`, `xe/`, and
root-level `iwlwifi-*` from the pinned Linux firmware package. The RK3566/Mali-G52
board has none of those PCIe GPU/radio devices. Intel Bluetooth and all other
firmware families remain. This is not the later board-only allowlist.

The read-only Linux 6.18.2 inventory identified RTL8821CS SDIO Wi-Fi and an
RTL8156 USB Ethernet adapter (USB ID `0bda:8156`). The connection used Ethernet;
Wi-Fi had SDIO errors and was disconnected. Bluetooth was not exposed. Keep
complete Realtek and Rockchip families until those paths have working baselines.

The device-scoped overlay changes only `linux-firmware`. Other packages selected
by `hardware.enableRedistributableFirmware`, the regulatory database, firmware
compression, drivers, recovery labels, and public SSH defaults are unchanged.
Odin does not import this overlay. Original firmware licenses are retained.

## Build-host verification

Run on Fuji, not on the handheld:

```sh
nix build .#checks.aarch64-linux.rg353m-firmware-phase1
```

The check rejects the untrimmed baseline, compares every retained file against
the original package by SHA256, checks directory/symlink preservation and target
resolution, and verifies the observed firmware families remain. The package also
rejects output references to its full upstream package and source checkout.

## Hardware gate

A successful build is not boot verification. Test a separate candidate SD, keeping
the verified recovery card unchanged and internal eMMC unmounted. If the candidate
uses the existing recovery labels, swap cards while powered off: do not insert
both labeled recovery cards simultaneously. No flash or boot operation is part
of these build checks. Validate boot, display, audio, USB Ethernet, Wi-Fi,
Bluetooth, and suspend/resume before proceeding to a narrower allowlist.
