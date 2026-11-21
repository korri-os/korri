# Thor internal system update, 2026-11-20 (ADR 0094)

Owner approval: given in chat for this one run ("Yes, just make sure you
rebase first"). Procedure: `nix/devices/thor/README.md`, "Manually update the
internal system".

## Source

`main` at `eff09b3a8` (ADR 0094: Velvet's clock, Wi-Fi and battery cluster;
korrid's battery from uevents and network from NetworkManager signals). Built
on Zao and signed with the builder key.

| | Path |
|---|---|
| Old system | `/nix/store/36zl37bwfp1bqmh2giva1f72dgy3pwk6-nixos-system-thor-sd-card-26.11.20260923.4975466` |
| New system | `/nix/store/wv4v4ysng5grax5ld9w0ir2b996lnpmd-nixos-system-thor-sd-card-26.11.20260923.4975466` |
| Old bundle | `/nix/store/cd09qs379h93y2hg54hqshrzx57n11lg-korri-bundle-0.0.0` |
| New bundle | `/nix/store/2hn4kc4652qfzm8h5bdcprayz9yjfl2z-korri-bundle-0.0.0` (in the new closure) |
| Old system kept as | `/nix/var/nix/profiles/system-thor-previous-adr0094` |

## Read-only checks before any write

| Check | Result |
|---|---|
| Model | `AYN Thor` |
| `/boot` | `/dev/sda17`, vfat, `THOR_ESP` |
| `/` | `/dev/sda18`, ext4, `THOR_ROOT` |
| SD card with `THOR_*` labels | none (`mmcblk0p1` is `GAMES`) |
| Free space | `/` 769,817,415,680 B; `/boot` 1,022,083,072 B |
| EFI variables | none listed (`e3b0c442…`), the same before and after staging |

## Steps

1. `nix copy` of the signed closure to `ssh-ng://root@192.168.1.236` (port 2222).
2. Old system set as `system-thor-previous-adr0094`, the system profile set to
   the new one, `switch-to-configuration boot` with `NIXOS_INSTALL_BOOTLOADER`
   and `NIXOS_INSTALL_GRUB` unset. Boot loader files unchanged.
3. `korri-bundle-select switch` to the new bundle.
4. `systemctl reboot`.

## After the reboot

| Check | Result |
|---|---|
| Booted system | the new one; kernel 7.2.0 |
| korrid, korri-shell | active |
| `KORRID_BATTERY_VERIFIED` in korrid's environment | `["percent"]` |
| power_supply | `battery` (qcom_battmgr): Battery, Charging, 100 % at the first read |
| Top display | Velvet home with the cluster at the top right: `12:29 AM`, Wi-Fi bars, a bolt, `99%`. The percent had moved from sysfs's 100 to 99 without a poll. |
| Failed units | `systemd-backlight@backlight:ae94000.dsi.0` (the bottom panel's backlight; not part of ADR 0094) |

## Not done

- Plugging and unplugging the charger by hand to time the bolt. Nobody was at
  the device.
- Screenshots of the pause panel and a list page; they need input on the
  device.
