# RG353M firmware reduction

Phase 2 retains complete `rtw88`, `rtl_bt`, `rtl_nic`, and `rockchip` families
from the pinned Linux firmware package, plus the signed wireless regulatory
database. These cover the inspected RTL8821CS Wi-Fi/Bluetooth, RTL8156 USB
Ethernet adapter, and Rockchip display firmware. This is not a promise of
support for arbitrary USB radios or other board variants.

The working firmware union overrides two `rtl_bt/rtl8761b` files through the
separate `rtl8761b-firmware` package. That package remains so the effective
family bytes do not change. The other generic firmware bundles are removed.
ALSA UCM profiles, audio drivers, Bluetooth configuration, kernel, recovery
labels and SSH policy do not change. Odin does not import this policy.

The upstream installed firmware output does not include its source notices.
The selected package now carries the pinned WHENCE and license/copyright texts
under `share/licenses/linux-firmware-rg353m` without retaining the source tree.

## Build-host verification

Run on Fuji, not on the handheld:

```sh
nix build .#checks.aarch64-linux.rg353m-firmware
```

The check rejects an untrimmed baseline, compares every retained package file,
checks internal symlinks and license text, and compares the effective NixOS
firmware union including the Realtek overrides and regulatory signatures.
Only the selected families and regulatory files can appear in that union.

## Hardware gate

The preceding candidate verified RTL8821CS firmware loading, Bluetooth discovery,
Wi-Fi association and scoped IPv6 traffic during discovery, and a timed wake
cycle. IPv4 with Ethernet and Wi-Fi on the same subnet still needs separate
routing/filter investigation. No firewall changes belong in this firmware cut.

Install a new prebuilt SD generation and retain the working generation for
rollback. Keep internal eMMC unmounted. Verify the same kernel, retained firmware,
SSH, default audio metadata, Wi-Fi and Bluetooth before accepting this cut.
No sound tests or permanent brightness changes are part of this phase.
