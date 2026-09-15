# Linux 7.2.6 SD cutover

The owner confirmed that the diagnostic SD image boots. A command through the
USB root console returned `7.2.6` from `uname -r`. This establishes boot and
console access, not peripheral acceptance.

## Changes and checks

The kernel uses upstream Linux 7.2.6 with the retained RK3326 configuration
seed, board DTS, generic panel driver and RK915 MMC quirks. The MMC patch
follows the newer DWMMC host structure. RK915 has a source patch for removed
kernel APIs; see `wifi/README.md`.

Native and cross kernel builds passed. The native RK915 build passed its
artifact checks, including 162 imported symbol CRCs. These Nix checks passed:

- `checks.x86_64-linux.r36tmax`
- `checks.x86_64-linux.r36tmax-mmc`
- `checks.x86_64-linux.r36tmax-recovery`

Built and written artifact:

```
/nix/store/x9qqcv5v74ynax00998q62s11zgn6i5c-nixos-r36t-max.img.zst/sd-image/nixos-r36t-max.img.zst
```

The owner approved erasing the removable card reporting 53,739,520,000 bytes.
The write completed successfully and flushed. The owner requested no readback;
there is no byte-for-byte card verification. Internal eMMC remained disabled.

## Wi-Fi on hardware: working

The owner's `rk915_fw.bin` and `rk915_patch.bin` went to `/lib/firmware` on the
card, and `/etc/korri/wifi.env` to mode 0600. Both blobs match the hashes in
`wifi/README.md`. The next boot reported:

```
RK915: INFO: rk915_download: start download firmware size: 48056
RK915: INFO: rk915_download: download firmware success
```

`wlan0` associated at -35 dBm, took a DHCP address, and moved 9.3 MB. NetworkManager
reports `wlan0:connected:korri`. This is the RK915 port's first working radio on
7.2.6. Throughput, roaming, power save and suspend remain untested.

Provisioning is per-card and is not in any image. The blobs have no
redistribution clearance; see `wifi/README.md`.

## Module unload hangs shutdown

A `systemctl reboot` never completed. Shutdown ran for 14 minutes, then stalled
with processes that survived `SIGKILL`, which means they were blocked in the
kernel:

```
firewall.service: Unit process 1605 (iptables) remains running after unit stopped.
dbus.service: Processes still around after SIGKILL. Ignoring.
```

The console's last line was `del rk915 device.` Only a power cycle recovered the
unit. An in-place `modprobe -r rk915 && modprobe rk915` fails the same way from
the other end, warning at `proc_register` and then `Failed to create proc dir`:
the driver's exit path leaves its `/proc` entries behind.

Until this is fixed, power-cycle the device instead of rebooting it, and do not
reload the module in place.

## Temperature: no 7.2 regression

Measured on this kernel with the trip points at 70C passive, 85C passive and
115C critical:

| Condition | zone0 |
| --- | --- |
| Kiosk running | 80.4-83.8 C |
| Idle, kiosk and compositor stopped | 55.0-56.8 C |
| 60 s all-core load | peaks 85.4 C, then steps to 1.2 GHz |

The earlier 52.5 C reading came from a console image with no browser, so idle at
55 C is the comparable number and shows no regression. Passive throttling works.

The kernel configuration selects `CONFIG_CPU_FREQ_DEFAULT_GOV_PERFORMANCE`, so
the clock holds 1.296 GHz even at idle. Switching to `schedutil` and
`simple_ondemand` was measured and produced no cooling under the kiosk, because
Chromium keeps real demand near the ceiling. The device was reverted to
`performance` and no configuration changed. Heat belongs to browser load.

## Remaining acceptance

Verify GPU rendering, buttons and audio on this kernel. Suspend, resume and
video codec jobs are not accepted by this boot result. Do not resume prior
faulting codec jobs without their separate recovery and test gates. Ask for
owner readiness before each hands-on test.

SSH over the USB cable works once the owner's public key is in
`/root/.ssh/authorized_keys`. The gadget serial console is
`/dev/serial/by-id/usb-Korri_R36T_Max_NixOS_r36tmax-nixos-if02`.
`systemd-networkd-wait-online.service` still fails at boot and keeps the system
`degraded`, even with Wi-Fi up, because NetworkManager owns `wlan0`.
