# OnePlus 6 (op6)

OnePlus 6 (enchilada, SDM845, 8 GB) with the Korri product: korrid, the
native shell on the panel, touch, the product USB gadget and the SSH plugin.
This is a spike. It boots the phone's existing kernel and boot.img.

## Where it comes from

The hardware support is the mwlaboratories Mobile NixOS fork
(`op6-mobile-nixos` in `flake.nix`, pinned to the revision that
phoneputer `ea5810e` locks). The phone ran phoneputer (NixOS 26.11, GNOME)
before Korri. GNOME, Stylix, Home Manager and Nixvim stay out.

| File | What it holds |
| --- | --- |
| `default.nix` | the system: the fork's `oneplus-enchilada` module, the Korri product, the image plugins, `hardware.nix`; the kernel; `op6-deploy` |
| `hardware.nix` | hardware facts: kernel, USB gadget, panel, touch, no sleep |
| `plugin-seed.nix` | the seeded plugin receipts for a phone with none (copy of the Tab S6's) |
| `deploy.sh` | the body of `op6-deploy` |

## Kernel

The kernel is the fork's sdm845-linus 7.4.0-rc3, the one already in boot_a
and boot_b. Mobile NixOS builds the device kernel from the system's nixpkgs,
which is Korri's here, and that would be a different store path. So
`default.nix` evaluates the kernel on phoneputer's nixpkgs (`op6-nixpkgs`)
with only the fork's modules and forces it into the Korri system. It is the
same path the phone boots:

```
/nix/store/1iq92v5mzip2sn4c83xm28fninr4bjrl-linux-aarch64-unknown-linux-gnu-7.4.0-rc3
```

Nothing rebuilds and the boot.img does not change, so a deploy never writes a
partition. The kernel already has Korri's delta
(`nix/devices/tabs6/kernel/config/korri.config`): checked in
`/proc/config.gz` on 2026-11-20, `INPUT_UINPUT=m`, `HIDRAW=y`, `UHID=m`,
`INPUT_JOYSTICK=y`, `JOYSTICK_XPAD=m`, `HID_PLAYSTATION`, `HID_SONY`,
`HID_NINTENDO`, `HID_STEAM` as modules and `NF_TABLES=y`.

## Network

| Route | Address | Port |
| --- | --- | --- |
| USB-C cable (Korri gadget, NCM, DHCP to the host) | `10.42.9.1` | 2222 (SSH plugin) |
| Wi-Fi (NetworkManager) | DHCP, was `192.168.1.186` | 2222 |
| ACM console on the cable | `/dev/ttyACM*` on the host | root autologin |

Only one gadget can own the controller `a600000.usb`.
`mobile.usb.stage-2.enable = false` removes the fork's stage-2 gadget, and
Korri's `usb-gadget.service` first removes the `g1` gadget that the fork's
stage-1 leaves behind. The fork's `172.16.42.1` exists only during stage-1
now.

The SSH plugin reads root's own `/root/.ssh/authorized_keys`. Port 22
(openssh) is off in the product.

Wi-Fi: NetworkManager keeps the profiles it has in
`/etc/NetworkManager/system-connections` (the `vrackie` profile from
phoneputer). The product's `korri` profile reads `/etc/korri/wifi.env`
(`nix/base/wifi.nix`). Put it there over stdin, never from a file in git:

```sh
ssh -p 2222 root@10.42.9.1 'install -d /etc/korri && umask 077 && cat > /etc/korri/wifi.env' < ~/.config/korri/wifi.env
```

## Deploy

On Zao. aarch64 parts go to the remote builders; nothing builds on the phone.

```sh
deploy=$(nix build --no-link --print-out-paths .#op6-deploy)/bin/op6-deploy
# Activate without changing the boot default (a reboot returns to the
# previous system):
$deploy --test root@10.42.9.1
# Make it the boot default and activate it:
$deploy --switch root@10.42.9.1
```

The first deploy from phoneputer (2026-11-20) copied the closure over port 22
at `172.16.42.1` with `--no-check-sigs`, because phoneputer did not trust
Korri's cache key (root is a trusted user there). A live `switch` from
phoneputer is refused (dbus-broker to dbus is a critical-component change),
so the first Korri start needs a reboot:

```sh
OP6_KNOWN_HOSTS=$HOME/.ssh/known_hosts.op6 $deploy --boot --no-check-sigs --port 22 root@172.16.42.1
ssh root@172.16.42.1 systemctl reboot     # comes back as "OnePlus 6 Korri" at 10.42.9.1:2222
```

A tethered `fastboot boot` of phoneputer's boot.img with an added
`mobile-nixos.generation=` argument did not work: stage-1 still booted the
phoneputer generation (the argument landed in the boot.img's extra command
line). That boot also crashed into Qualcomm download mode (`05c6:900e`)
about 17 s in, while the DSPs loaded firmware, so the plain Mobile NixOS boot
is not fully stable either. Use `--boot` and a normal reboot.

It also installs the seeded plugin receipts (the SSH plugin among them).

## Recovery

| Problem | Do this |
| --- | --- |
| a `--test` activation broke SSH | hold Power about 10 s to restart; the boot default is unchanged |
| a `--switch` system is bad, SSH works | `nix-env -p /nix/var/nix/profiles/system --rollback && /nix/var/nix/profiles/system/bin/switch-to-configuration switch` |
| a `--switch` system is bad, no SSH | restart; the Mobile NixOS boot menu (stage-1 recovery, Volume keys) lists the older generations, among them phoneputer |
| the phone does not boot at all | power off, Volume Up + Power for fastboot, then `fastboot -s 2d7ef017 flash --slot=all boot /tmp/forgerie-scratch/f-b6fd2e05/op6-images/boot.img` (phoneputer's boot.img) |

Write only `boot_a`, `boot_b` and `userdata` (and erase `dtbo`). Never write
`xbl*`, `abl*`, `tz*`, `hyp*`, `persist`, `modemst*`, `fsg` or any other
firmware partition.

## What does not work yet

Next steps, not done in the spike:

- U-Boot or systemd-boot instead of the Android boot.img chain.
- A product kernel of its own (a change needs a new boot.img: try it with
  `fastboot boot`, flash both slots only after it works).
- USB OTG (VBUS) gamepads, Bluetooth controllers.
- Sleep. The power key powers off (the product default).
- Modem, cameras, audio tuning, sensors, the alert slider.
- Volume keys as shell controls; landscape rotation of the shell.
- A module check like `nix/devices/tabs6/module-check.nix`.
