# Thor internal system update, 2026-11-19 (ADR 0086)

Owner approval: given in chat for this one run, after the read-only checks
were described. Procedure: `nix/devices/thor/README.md`, "Manually update the
internal system", which follows the Odin's "Manually update an approved
system".

## Source

`main` at `e157ca898` (ADR 0086: surface panes, and the fix to Velvet's Floor
list page). Built on the configured builders and signed on Zao.

| | Path |
|---|---|
| Old system | `/nix/store/x04dbpp0lva7y2khxdsx510avw68r7cz-nixos-system-thor-sd-card-26.11.20260923.4975466` (generation 4) |
| New system | `/nix/store/3ja2gbqq7kv8863ihn3avipcsflq6in7-nixos-system-thor-sd-card-26.11.20260923.4975466` |
| Old bundle | `/nix/store/ylgvxwj7yjxkwwqf73y3s32q4igzxsgb-korri-bundle-0.0.0` |
| New bundle | `/nix/store/bwiwpyym4j6lkn141pysahnrg197hsaf-korri-bundle-0.0.0` (in the new closure) |
| Old system kept as | `/nix/var/nix/profiles/system-thor-previous-adr0086` |

## Read-only checks before any write

| Check | Result |
|---|---|
| Model | `AYN Thor` |
| `/boot` | `/dev/sda17`, vfat, `THOR_ESP` |
| `/` | `/dev/sda18`, ext4, `THOR_ROOT` |
| SD card with `THOR_*` labels | none |
| Free space | `/` 879,935,586,304 B; `/boot` 955,351,040 B |
| EFI variables | none listed (`sha256` of the listing `e3b0c442…`), the same before and after staging |

## Steps

1. `nix store sign --recursive` with the builder key on Zao; `nix copy` to
   `ssh-ng://root@192.168.1.236` (port 2222).
2. On the device: the old system set as `system-thor-previous-adr0086`, the
   system profile set to the new one, then `switch-to-configuration boot` with
   `NIXOS_INSTALL_BOOTLOADER` and `NIXOS_INSTALL_GRUB` unset. The boot loader
   files were unchanged ("same boot loader version"). The new entry is the
   default; the old system's entry and `thor-main-top-only.conf` remain.
3. `korri-bundle-select switch` to the new bundle.
4. `systemctl reboot`.

## After the reboot

| Check | Result |
|---|---|
| Booted system | the new one; kernel 7.2.0 |
| korrid | `/nix/store/7lrxcw9g6nsdsnzzlsdxbwb6awnqickb-korrid-0.0.0/bin/korrid`, from the new bundle |
| korri-shell | `/nix/store/z3bcvn7ayi5iqpz8pzqk1cz4y3zayj8p-korri-shell-0.0.0`, surfaces `/nix/store/7vxfvvin0hilkjx58mhsd16jspbwns0x-korri-shell-surfaces` (Velvet declares `stage` over `floor`) |
| Failed units | none (korri-shell waited on its dependencies once at boot, then started) |
| Top display | Velvet's home in one window: the pick and the row under it, as before ADR 0086. Two screenshots 2 s apart differ (the stage is drawing). |
| Whole-boot counters | `failed wait_for_idle` 0, `frame done timeout` 6 (the boot-time `enc35` timeouts the README records), `Page-flip failed` 50 |

## Not done

Two displays at runtime. The image declares only DSI-2, so korrid gives
Velvet's Stage and Floor one window. Re-plugging DSI-1 live was not tried:
the Thor README says not to enable DSI-1 live on the 7.2 kernel, because it
wedged the top screen. Two displays on the Thor wait for #526.

Only a person can check: touch on both displays, the look of the dimmer
stage light on the Floor, and the AYN key once #526 lands.

## Rollback

Restore the old bundle with `korri-bundle-select switch` to the old bundle,
set the system profile to the old system, run its `switch-to-configuration
boot` with the install-bootloader variables unset, and reboot.
