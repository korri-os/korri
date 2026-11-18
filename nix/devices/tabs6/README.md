# Samsung Galaxy Tab S6 (SM-T860, gts6lwifi, SM8150)

This directory builds two NixOS systems for the Galaxy Tab S6 Wi-Fi:

- `consoleConfiguration` is the tabs6 Linux console that boots on the tablet
  today: simon's sway + foot + tmux session, the RNDIS + ACM USB lifeline at
  `172.16.42.1`, standalone iwd Wi-Fi, PipeWire audio, panel brightness and
  idle dim, idle suspend on battery, and the ESP tooling.
- `configuration` (`nixosConfigurations.tabs6`) extends it with Korri's
  product module and image plugins for device `tabs6`. The native shell is
  the screen. It is a **development image**: it builds and passes the
  evaluation gates; nothing of the product has run on the tablet yet.

Both boot the same way: Samsung ABL, Project Aloha UEFI in `boot`,
systemd-boot on a FAT ESP on `cache`, the EFI-stub kernel with the gts6lwifi
DTB, root on `userdata`, no initrd.

## Where it came from

Moved from the tabs6 repository (`/home/simonwjackson/code/sandbox/tabs6`,
commit `97c2734`, 2026-11-17). Its `AGENTS.md` holds the device facts, the
flashing history and the hard partition rules; they still apply. Names such as
`PORT.md`, `USB_HOST.md` or `DAILY_DRIVER.md` in comments are documents of the
reference port, [Bentlybro/gts6l-mainline-linux](https://github.com/Bentlybro/gts6l-mainline-linux).

| Here | tabs6 repository | Change |
| --- | --- | --- |
| `kernel/` | `nix/kernel/` | `productFragment` and `config/korri.config` added; the console kernel is the same derivation |
| `firmware/` | `nix/system/firmware/` | none |
| `esp/` | `nix/system/esp/` | none |
| `images/` | `nix/system/images/` | rootfs carries the plugin seed; deploy signs, picks a port, seeds plugins once; `esp-installer` left behind |
| `system/` | `nix/system/` | `tabs6.console.enable` gates the console's own session, lifeline and network; `kernel.nix` loses the placeholder kernel |
| `usb-role/` | port `tools/usb-role` | new NixOS module, test |
| `bluetooth/` | — | new: `tabs6-bt-pair` |
| `hardware.nix`, `module-check.nix` | — | product facts and gates |

Not moved, still only in the tabs6 repository: `nix/bootimg` (the Samsung boot
image with Aloha), `nix/installer` and its ESP variant, and `scripts/`
(Odin/fastboot flashing, diagnostics). Flashing is parent-only work there.

The kernel and the firmware build from the `tabs6-nixpkgs` flake input, the
nixpkgs the tablet was proven with (Linux 7.3.4); the systems use Korri's
nixpkgs. The console therefore runs older userspace packages than the tabs6
repository's build (Korri's pin is from 2026-09-23).

## Korri parts reused and tabs6 parts kept

| Need | Used | Why |
| --- | --- | --- |
| USB gadget | Korri `usb-gadget` (NCM + ACM, `10.42.7.1`) | Fits: the kernel has configfs NCM/ACM built in. The tabs6 RNDIS gadget stays on the console only |
| USB role and VBUS | tabs6 `usb-role/` | No Korri module knows role switches or VBUS: every other device has a fixed peripheral port |
| Backlight step | Korri `korri-backlight-step` units | Fits: it steps any `/sys/class/backlight` device by percent; the panel has 0-45 levels |
| Display idle | Korri `korri-display-idle` (5 min off through korrid) | Fits. The console's 2-minute dim stays a console feature |
| udev coldplug | Korri (product) | Fits; nothing tabs6-specific |
| Sleep | Korri: no sleep state, power key powers off | The console's s2idle idle suspend has no session freeze; it stays console-only |
| Wi-Fi | Korri NetworkManager + wpa_supplicant | The product gate requires wpa_supplicant and korrid's Wi-Fi view uses NetworkManager. The modem chain (rmtfs, tqftpserv, `tabs6-wifi`) and the efs address are kept |
| SSH | Korri `@korri:ssh` plugin, key-only, port 2222 | The product forbids `services.openssh` |
| Bluetooth | `hardware.bluetooth` + tabs6 `tabs6-bt-addr` | Korri has no Bluetooth module beyond enabling BlueZ (`rg353m/bluetooth.nix`) |
| Kernel, boot, ESP, firmware, charge, audio UCM, zram, UFS write-through | tabs6 | Board support |

## Input on the product

The shell never opens `/dev/input` and gets input two ways:

| Source | Path | State |
| --- | --- | --- |
| Bluetooth controller | BlueZ → evdev → InputPlumber profile → inputd → korrid native input → shell buttons | Primary. Works for controllers InputPlumber adopts (below). Unverified on the tablet |
| Touchscreen | fts1ba90a → libinput in the Korri compositor → Wayland touch → Slint | Surfaces get touch where their views take pointer input; the shell itself maps only "touch started" |
| USB gamepad | USB-C in host role (`tabs6-usb-role host`) → same as Bluetooth | Manual switch only; cuts the USB lifeline while in host role |
| Volume keys | gpio-keys / PMIC resin → compositor | Do nothing: no InputPlumber data for this board routes them to inputd's volume actions |
| Power key | logind | Powers the tablet off (product policy without a sleep state) |

Controllers InputPlumber adopts (`services/inputd/nix/inputplumber-data.nix`):
Xbox 360 / One / Elite over USB, Switch Pro, 8BitDo Pro 2, Logitech Dual
Action, and the generic gamepad profile. Korri deliberately **ignores**
DualShock 4, DualSense, DualSense Edge and the Xbox One/Series **Bluetooth**
profile until inputd can bind their multi-interface devices. Those controllers
pair but do not drive the shell.

## Pair a Bluetooth controller

Korri has no pairing screen. Put the controller in pairing mode, then run once
over SSH:

```sh
ssh -p 2222 root@10.42.7.1 tabs6-bt-pair             # pairs the one controller found
ssh -p 2222 root@10.42.7.1 tabs6-bt-pair AA:BB:CC:DD:EE:FF
```

Over Wi-Fi use the tablet's address (`tabs6.local`). It pairs, trusts and
connects. A trusted controller reconnects by itself when switched on.

## USB-C: lifeline or gamepad

The one USB-C port is the gadget lifeline (device role) by default, on every
boot. `tabs6-usb-role` changes it by hand:

```sh
tabs6-usb-role            # role, VBUS, attached devices
tabs6-usb-role host       # power the port (SM5705 boost, 5.1 V, 900 mA), then host
tabs6-usb-role device     # gadget again, VBUS off, rebind the gadget unit
```

It refuses to power the port unless the fuel gauge says `Discharging` and no
USB host has the gadget configured. Run `host` over Wi-Fi SSH: it ends a USB
SSH session. Boot always returns to device role with the boost off
(`tabs6-usb-role-boot*.service`). `usb-role/usb-role.test.py` exercises all of
this against a temporary sysfs tree and register file; no hardware has run it.

Proposed automatic policy (not built): hold device role for 60 s after boot;
while a host has the gadget configured or anything powers the port, stay
device; after 30 s of neither, power the port and become host for at most
10 s; stay host while a USB device is attached; go back to device when it
leaves or nothing enumerates. Its cost: with no Type-C port manager the tablet
cannot see a gamepad before it powers the port, so it must probe, and a
computer plugged in during a probe meets the tablet's own 5 V for up to 10 s.
A driver for the s2mm005 PD controller (attach detection) would remove the
probe.

## Build

On a Linux build host, never on the tablet:

```sh
bash .github/workflows/fetch-image-plugins.sh tabs6     # once: the pinned plugin archives
nix build .#packages.x86_64-linux.tabs6-toplevel .#packages.x86_64-linux.tabs6-console-toplevel
nix build .#packages.x86_64-linux.tabs6-deploy-over-ssh
nix build .#checks.x86_64-linux.tabs6 .#checks.x86_64-linux.tabs6-usb-role .#checks.x86_64-linux.tabs6-esp
```

The firmware, the images and the deploy tools need the three stock firmware
members in the local store (never in git, never in a cache):

```sh
nix run .#tabs6-stage-firmware -- /home/simonwjackson/code/sandbox/tabs6/firmware
```

## Deploy

`deploy-over-ssh` copies the closure, syncs `/lib/firmware`, runs
`switch-to-configuration boot` and writes the ESP through `tabs6-esp`. It
never reboots unless asked and never flashes.

| Tool | Default target | Signing |
| --- | --- | --- |
| `tabs6-deploy-over-ssh` (product) | `root@10.42.7.1`, port 2222 | Zao's cache key, signatures checked |
| `tabs6-console-deploy-over-ssh` | `root@172.16.42.1`, port 22 | unsigned, `--no-check-sigs` (as before) |

### First product deploy (from the console)

The running console has openssh on 22 and the RNDIS lifeline. The product has
neither: it has the NCM gadget at `10.42.7.1` and the SSH plugin on 2222,
which reads root's own `authorized_keys`. Before the switch, on Zao:

```sh
# 1. Root's key for the SSH plugin (the console keeps its key in /etc only).
ssh root@172.16.42.1 'install -d -m700 /root/.ssh && cat /etc/ssh/authorized_keys.d/root >> /root/.ssh/authorized_keys && chmod 600 /root/.ssh/authorized_keys'
# 2. Wi-Fi for NetworkManager (iwd's saved networks do not carry over).
#    /etc/korri/wifi.env holds WIFI_SSID= and WIFI_PSK= (nix/base/wifi.nix).
# 3. Deploy and reboot.
$(nix build --no-link --print-out-paths .#tabs6-deploy-over-ssh)/bin/deploy-over-ssh --port 22 --reboot root@172.16.42.1
```

The first deploy also installs the plugin receipts (the SSH plugin among
them) because the console has none. After the reboot: `ssh -p 2222
root@10.42.7.1`, then `tabs6-esp mark-good` once the product has survived a
reboot. `tabs6-esp rollback` (or volume-down at power-on) returns to the
console entry.
