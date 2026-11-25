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
commit `97c2734`, 2026-11-17). That repository is now archived at
`archived-2026-11-24`; this directory is the maintained device tree.
[RECOVERY.md](RECOVERY.md) holds the flashing procedure, hard partition rules
and parent-only flashing policy. Read it before any boot or storage work.
[ARCHIVE.md](ARCHIVE.md) records the inventory and what was kept or superseded.
Names such as
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

The rejected boot-image recipe and superseded installer remain archived, not
active Korri build paths. Recovery uses the accepted v2 boot archive and Mu
fastboot; see [RECOVERY.md](RECOVERY.md). Useful diagnostics now live in
`scripts/`; [ARCHIVE.md](ARCHIVE.md#diagnostics) explains their scope.

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
| USB gamepad | USB-C in host role (`tabs6-usb-role host`) → same as Bluetooth | Automatic with the permanently attached Kishi V2; cuts the USB lifeline in host role |
| Volume keys | gpio-keys (up) / PMIC resin (down) → inputd system keys → product volume actions (wpctl ±5 %) | Volume works without a controller. Home+volume changes brightness with any controller. inputd combines local sources; no compositor binding or keys-only InputPlumber composite is needed. |
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

The Korri product kernel owns the USB-C data role through the read-only
`s2mm005-read-only` driver. Three fresh qualifying samples select host while
the Kishi pass-through powers the tablet, without enabling boost. Charger
unplug selects device on the next sample. The standard `/sys/class/typec`
port exposes the observed attachment and roles. See `usb-role/typec-kernel.md`
for exact interlocks and limits. The console retains manual role control.

```sh
tabs6-usb-role                 # role, VBUS, attached devices
tabs6-usb-role host            # stop watcher; unbind driver; existing manual battery-host checks
tabs6-usb-role device          # verify boost off, restore gadget, rebind automatic kernel policy
```

Manual host still requires Discharging, three source samples, no incoming
VBUS and no configured gadget. It is an unmonitored operator override: select
`device` before changing cables. A failed manual attempt stays unbound until
`device` verifies boost-off and restores policy. Reboot also restores policy.

There is no automatic user-space role writer or chip-status daemon.
`tabs6-typec-role-watch` watches only the selected role: it stops the gadget
in host role and restarts it on return to device. It never writes chip
registers or the role, and it does not restart input providers. The old Phase A diagnostic observer is not
started and works only with the kernel driver unbound. Existing charge-current
maintenance and boot boost-off remain unchanged.

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

On zao, staging registers one **indirect GC root per stock member** under
`${XDG_STATE_HOME:-$HOME/.local/state}/korri/gcroots/tabs6-firmware/`:
`NON-HLOS.bin.lz4`, `modem.bin.lz4`, and `vendor.img.ext4.lz4`. Each symlink
points to the fixed-output store path used by `requireFile`; Nix registers
its stable location in `/nix/var/nix/gcroots/auto`. Rerunning staging safely
refreshes these roots. They survive worktree removal and ordinary GC. Do not
move or delete them while tabs6 builds are needed. This costs store space for
all three compressed inputs; it does not root derived firmware outputs.

The proprietary `firmware/` directory stays in the archived tabs6 checkout at
the absolute path above. Never stage it in git or copy it to a cache. The
archive tag contains source and diagnostics only, not these inputs.

Verify on the build host without deleting anything:

```sh
roots="${XDG_STATE_HOME:-$HOME/.local/state}/korri/gcroots/tabs6-firmware"
readlink "$roots"/* > /tmp/tabs6-firmware-inputs
nix-store --gc --print-dead > /tmp/tabs6-dead-paths
# Must print nothing (grep exits 1 when none match).
grep -Fxf /tmp/tabs6-firmware-inputs /tmp/tabs6-dead-paths
nix eval .#tabs6-deploy-over-ssh.drvPath
# Focused host integration test: tiny public fixtures and real indirect roots.
python3 nix/devices/tabs6/firmware/stage-stock-firmware.test.py
```

Use `--print-dead` only for this audit, **not** a real GC. A different build
host must stage and root its own local inputs.

## Deploy

`deploy-over-ssh` copies the closure, syncs `/lib/firmware`, activates the
system and writes the ESP through `tabs6-esp`. It never reboots unless asked
and never flashes. Without flags, the product switches the running system
and its bundle together; the console still stages a boot. With `--reboot`,
the product stages a boot, reboots, then selects the new system's bundle.

System activation never changes the selected Korri service bundle, so the
product deploy selects the system's own bundle
(`services.korriBundle.initialPackage`) with `korri-bundle-select switch`, as
`nix/devices/rgds/UPDATE.md` does by hand. Without `--reboot` (including `--switch`) it selects at once.
With `--reboot` it selects as soon as the new system is up (the trial is
blessed only once korrid runs, and an old korrid can crash-loop under new
units), checks that korrid runs the bundle's binary, then waits for
`tabs6-boot-good` to log `blessed:`. A boot that fell back to LAST KNOWN GOOD
gets its previous bundle back, and the deploy exits non-zero. It verifies
both the active bundle symlink and the running korrid executable, including
when the symlink already matches.

Before 2026-11-22, a product deploy without flags only staged a boot and
printed a manual bundle-selection command. The Kishi deploy took that path;
a later reboot could therefore run new units with the old bundle. A product
deploy no longer leaves bundle selection as a manual follow-up.

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

## Offline root check

`userdata` is the ext4 root and the tablet has no initrd, so e2fsck can only
repair it from a boot that keeps root read-only. On the tablet, from a blessed
system:

```sh
tabs6-esp fsck-once && systemctl reboot   # one boot: e2fsck -fy userdata, then the normal entry
tabs6-esp fsck-log                        # after it: the check's log, kept on the ESP
```

The check entry is counted (one try). Its PID 1, `tabs6-fsck-init`, runs
e2fsck, writes the log and the normal `loader.conf` back through
`tabs6-esp fsck-done`, and reboots. If it never gets that far, systemd-boot
skips the spent entry and boots `tabs6-current`; `tabs6-esp install` also
drops a pending check. Proven on 2026-11-18: the first run repaired 2213
recorded errors (the log was lost to a bug fixed since), the second logged
`e2fsck exit 0`.
