# AYN Thor

`nix build .#thor-sd-image` produces `nixos-thor.img.zst`, a removable-media
NixOS image for the AYN Thor (Qualcomm SM8550 / QCS8550, two AMOLED screens).
It boots to Korri's shell on the top screen with korrid and key-only SSH, as
the Odin 2 Portal image does. It writes nothing to internal storage, to a
firmware partition (`abl_*`, `loader_*`, `xbl*`, `tz*`, `hyp*`, `aop*`,
`devcfg*`, `uefi*`, …) or to the bootloader lock state.

**Status (verified on the owner's Thor, 2026-11-18):** the image boots from
the SD card and, after a separate owner-approved run, from internal UFS. The
kernel is 7.2, `systemctl is-system-running` reports `running` with no failed
units, Wi-Fi joined through the shell, and the owner started a game. The
open faults are in [Hardware results](#hardware-results-2026-11-18) and
[TODO](#todo-needs-the-device).

## Boot chain

The Portal's chain, unchanged (`../odin2portal/sd-image.nix`, imported by
`./sd-image.nix`):

```
PBL -> XBL -> ABL (BOOT MODE = Loader) -> loader_a (AYN U-Boot) ->
bootefi bootmgr -> EFI/BOOT/BOOTAA64.EFI on the card -> systemd-boot ->
kernel + initrd + devicetree qcom/qcs8550-ayn-thor.dtb
```

The device facts for this chain (Android 13, build
Thor_V1.0.0.360_20260112, slot `_a`, bootloader unlocked, `loader_a`
present) were checked by the owner on 2026-11-18. The chain itself was
observed the same day: BOOT MODE = Loader -> `loader_a` U-Boot -> the SD
card's ESP -> systemd-boot -> Linux 7.2. Later boots use the same chain with
the internal ESP.

| Partition | Type | Label |
|---|---|---|
| 1 | GPT EFI System (ef00), vfat, 1 GiB | `THOR_ESP` |
| 2 | GPT Linux root arm64 (8305), ext4 | `THOR_ROOT` |

Root is mounted by `THOR_ROOT`, `/boot` by `THOR_ESP`. Both differ from the
Portal's internal `ODIN2P_ESP` / `ODIN2P_ROOT` and from ROCKNIX's `STORAGE`.
On first boot the root grows to the end of the card (`nix/formats/expand-root.nix`
acts only on the device that holds `/`). No unit mounts or scans `sda*`
(internal UFS).

**Label collision.** The owner's internal install (below) uses the same
`THOR_ESP` / `THOR_ROOT` labels as this image. With both present, `/` and
`/boot` can resolve to either medium. Do not insert a card made from this
image into the installed Thor. A rescue SD needs labels of its own and root
selection by them; that image is a follow-up and is not built.

## Owner's internal installation

Owner-approved, one run, 2026-11-18. Only UFS LUN 0 (`/dev/sda`) changed:

| Partition | Before | After | Sectors |
|---|---|---|---|
| `sda17` | `userdata` (1957791-248838138) | `THOR_ESP`, EF00, vfat | 1957888-2220031 |
| `sda18` | none | `THOR_ROOT`, 8305, ext4 | 2220032-248838138 |

Both were filled by rsync from the running SD system. `sda1`-`sda16` are
unchanged. LUN 1 to 5 (`sdb`-`sdf`: `xbl*`, `abl*`, `loader*`, `tz*`,
`hyp*`, `aop*`, `devcfg*`, `uefi*`, `uefivarstore`) were never written.
Android no longer boots, because `userdata` is gone. The bootloader stays
unlocked and `loader_a` is unchanged, so the AYN boot menu and BOOT MODE
switch still work.

## Manually update the internal system

**Every write to internal UFS needs explicit owner approval for that run.
The 2026-11-18 approval covered that one install only. Never turn this into
an automated internal-storage update path.** Coordinate with other workers
before any write.

Build and sign on Zao or the configured builders, never on the handheld.
Before copying or changing anything, check on the device, read-only:

```sh
tr -d '\0' </proc/device-tree/model          # AYN Thor
readlink -f /run/current-system
findmnt --mountpoint /boot                     # /dev/sda17, vfat, THOR_ESP
findmnt --mountpoint /                         # /dev/sda18, ext4, THOR_ROOT
blkid -p -s LABEL -o value /dev/sda17 /dev/sda18
lsblk -o NAME,LABEL,MOUNTPOINT                 # no SD card with THOR_* labels
bootctl --no-pager status
df -B1 / /boot
```

Refuse the update if the model, the devices, the types or the labels differ
from the table above, or if a card with `THOR_*` labels is inserted. Labels
alone are not sufficient. Then follow the Odin's procedure
(`../odin2portal/README.md`, "Manually update an approved system"): copy the
signed closure, set the system profile, run `switch-to-configuration boot`
with the install-bootloader variables unset, keep the previous generation,
and reboot explicitly. `efi.canTouchEfiVariables` stays false.

No step writes a raw block device, the GPT, LUN 1 to 5, or any firmware
partition, changes the bootloader state, or writes EFI variables.

## What is shared with the Portal

| Piece | Source | Thor change |
|---|---|---|
| Kernel 7.2 + ROCKNIX SM8550 queue + `.config` | `../odin2portal/kernel` (cross-built) | none; the same derivation |
| DTB | `./kernel`, built from the Odin kernel's `src`, `patches` and `configfile` | Thor DTS only |
| Firmware | `../odin2portal/firmware` | `variant = "thor"` |
| ROCKNIX SM8550 userspace (hwdb, PipeWire quantum) | `../odin2portal/rocknix` | none |
| SD image, ESP layout, GPT conversion, initrd, platform policy | `../odin2portal/sd-image.nix` | labels, DTB, host name; no 7.0.2 rescue entry |
| InputPlumber capability maps | `../odin2portal/rocknix/inputplumber/capability_maps` | Thor composite in `./inputplumber` |

The Portal's `toplevel` derivation is unchanged by this work except for
`KORRI_BUILD_ID`, which follows the commit.

## Kernel

The Thor DTS comes from ROCKNIX rev 1178bc2238de782bf081c558c177d35bb3690021
(the Odin kernel's rev), path
`projects/ROCKNIX/devices/SM8550/linux/dts/qcom/qcs8550-ayn-thor.dts`,
unchanged. Its `qcs8550-ayn-common.dtsi` is the Odin tree's copy, which is
byte-identical to that rev.

Every Thor DTB compatible has a driver in the shared config:

| DTS node | Compatible | Config |
|---|---|---|
| top panel (mdss_dsi1, `display_panel`) | `chipone,icna3520` | `DRM_PANEL_CHIPONE_ICNA35XX=y` |
| bottom panel (mdss_dsi0) | `ch13726a,thor` | `DRM_PANEL_DDIC_CH13726A=y` |
| top touch (i2c4 @0x38) | `focaltech,ft5426` | `TOUCHSCREEN_EDT_FT5X06=y` |
| bottom touch (i2c_hub_3 @0x38) | `focaltech,ft5452` | `TOUCHSCREEN_EDT_FT5X06=y` (+ patch 0053 `no-regmap-bulk-read`) |
| controller MCU | `gamepad,rsinput` | `JOYSTICK_RSINPUT=y` |
| AYN key, lid | `gpio-keys` | `KEYBOARD_GPIO=y` |
| shoulder LEDs | `heroic,htr3212`, `leds-group-multicolor` | `LEDS_HTR3212=y`, `LEDS_GROUP_MULTICOLOR=y` |

Nothing is missing for the first slice. `0000-version-0010-msm-resource-cleanup.patch`
stays disabled, as for the Portal.

## Hardware facts and where they come from

| Fact | Value | Source |
|---|---|---|
| Top screen connector | `DSI-2` | shown the shell on this image, Linux 7.2, 2026-11-18; earlier under ROCKNIX (nix-on-rocks `guest/modules/chipsets/sm8550/default.nix`, 2026-05-08) |
| Top screen panel | icna3520, 1080x1920, DT `rotation = <90>` | pinned DTS |
| Top screen mode | `1080x1920@120Hz` | first mode of `icna3520_modes`, kernel patch 0057; the 120 Hz link is unproven, see [Hardware results](#hardware-results-2026-11-18) |
| Top screen transform | 90 | observed under ROCKNIX (same source as the connector); the shell showed on 7.2 with it |
| Bottom screen | `DSI-1`, `ch13726a,thor` (command mode) on `ae94000`, 1080x1240 | connector and controller in the 2026-11-18 kernel log; pinned DTS and patch 0057 (panel, mode) |
| Touch input names | `top_touchscreen`, `bottom_touchscreen` | DT `label` + kernel patch 0015 (the DTS has no `input-name`); not observed |
| Gamepad | `AYN Odin2 Gamepad`, `rsinput-gamepad/input0` | `gamepad-name` in the shared dtsi, not overridden for the Thor; not observed |
| Volume keys | `gpio-keys` (up), `pmic_resin` (down) | shared dtsi; not observed on the Thor |
| AYN key / lid | `KEY_F24` on `gpio-keys-ayn`; `SW_LID` on `gpio-keys-lid` | pinned DTS |
| ADSP, amp firmware | `qcom/sm8550/ayn/thor/*`, `AYN-Thor-tplg.bin` | DTS `firmware-name`; ROCKNIX rev f080b462 (the firmware package's rev) |
| Fan | `pwmfan`, `cpu7-top-thermal`, `thor-whisper` | measured on the Thor by legacy (d0e5804d0) |
| Kernel command line | ROCKNIX SM8550 `EXTRA_CMDLINE` | `projects/ROCKNIX/devices/SM8550/options` at 1178bc22; the Portal's `platform-policy.nix` carries it |

Firmware notes: the Thor's ADSP images and JSON manifests are byte-identical
to the Portal's at f080b462; only `aw883xx_acf.bin` differs. The WCN7850
`board-2.bin` is shared by the SM8550 tree, so there is no Thor-specific
Wi-Fi board file to choose.

## First slice: what runs and what is deferred

| Area | State |
|---|---|
| Boot, top screen, Korri shell, korrid | in |
| Controller input | in: InputPlumber composite for model `AYN Thor`, one `xb360` target, both volume keys through DBus |
| Wi-Fi (ath12k WCN7850), SSH plugin, USB gadget `10.42.8.1` | in |
| Bottom screen | off: Sway `output DSI-1 power off` at start; its touch controller is ignored by libinput (`LIBINPUT_IGNORE_DEVICE`) |
| AYN key (`KEY_F24`), Back (`KEY_BACK`) | unbound |
| Fan | in (shared curve) |
| Audio | deferred, see TODO |
| Suspend | none (`sleep.states = [ ]`); power key and lid close power off, as on the Portal |
| Shoulder LEDs, haptics | untouched |

## Hardware results (2026-11-18)

Observed by the owner and from the device's kernel log on the owner's Thor,
this image (commit f950545cc), Linux 7.2.

- **Top screen (DSI-2, 1080x1920@120Hz).** On the first SD boot DSI-2
  stayed black. The display controller (SMMU SID `0x1c00`,
  `display-subsystem@ae00000`) logged `Unhandled context fault` at each
  commit. A live `swaymsg output DSI-2 mode 1080x1920@60Hz` brought the
  picture up. On the internal boot at 120 Hz the shell showed directly, with
  20 such faults. So the 120 Hz link is unproven, not proven broken. The
  image keeps 120 Hz; the 60 Hz command is the workaround.
- **Bottom screen (DSI-1).** A live `output DSI-1 power on` gave no picture
  on DSI-1. It logged `dsi_err_worker: status=d/c`,
  `dpu_encoder_frame_done_timeout` on enc38, then
  `failed wait_for_idle ... ret:-110 pp:0`, and it garbled the **top**
  screen until DSI-1 was powered off again. The image keeps DSI-1 off.
  **Never power DSI-1 on live** until a bottom-screen slice fixes this.
- **First boot.** 50 SMMU faults on SID `0x1c00` at 0.75 s come from the
  bootloader splash handoff. One `WARNING: mm/shrinker.c:212` from udevadm
  at 165 s. The shell start race is listed under TODO.

## TODO (needs the device)

- **Display idle wedge.** Korri's display idle powers outputs off after
  300 s (`output DSI-2 power off`). In May 2026, under ROCKNIX's kernel of
  the time, powering DSI-2 off wedged it with `dsi_err_worker` FIFO
  underflows until a reboot (nix-on-rocks
  `docs/brainstorms/2026-05-07-002-rocknix-thin-host-nix-main-space.md`).
  Unverified on 7.2; the Portal idles its icna3512 with the same driver
  family. First-boot test: leave the device idle for more than 300 s, wake
  it, and check `journalctl -k` for `dsi_err_worker` or underflow lines.
  Recovery: SSH, or the power button (powers off).
- **Top touch orientation.** The Thor DTS sets `touchscreen-swapped-x-y`
  and `touchscreen-inverted-x`; legacy needed `calibration_matrix 0 -1 1 1 0 0`
  under its kernel. Unknown which applies here; this image maps touch to
  DSI-2 with no matrix. Tap the corners and correct.
- **Input names.** Confirm `AYN Odin2 Gamepad`, `gpio-keys`, `pmic_resin`,
  `top_touchscreen` and `bottom_touchscreen` with `libinput list-devices`.
- **Top screen at 120 Hz.** See the hardware results. If DSI-2 stays
  black, bring it up live with `swaymsg output DSI-2 mode 1080x1920@60Hz`
  and record the faults.
- **Setup, Names step.** The owner reported that A on the empty Name field
  did nothing, and bypassed the step. Not diagnosed: a raw evdev capture is
  blocked because InputPlumber and inputd grab the devices. Open.
- **Shell start race.** On the first boot `korri-shell` failed once with
  `korrid is not listening`: korrid started at 21:48:25, after the shell's
  timeout. A restart recovered it. Not seen on later boots; not fixed.
- **Rescue SD.** Distinct labels and root selection, see the label
  collision above.
- **Audio.** The card should be `AYNThor` (DTS `model = "AYN-Thor"`), WCD938x
  plus two AW883xx amps. The image carries the Portal's UCM tree, whose
  profile does not match this card. The sink is unresolved: nix-on-rocks
  `thor.nix` and legacy 945934d96 say `HiFi__Speaker__sink`, legacy
  work/task-010 says `pro-output-0`. Read the longname from
  `/proc/asound/cards`; the UCM file must be `conf.d/sm8550/<longname>.conf`
  exactly, with no trailing dash.
- **Bottom screen.** Never enable DSI-1 live (see the hardware results).
  Enabling it needs a bottom-screen slice that fixes the DSI-1 command-mode
  path first, then a design for two displays in Korri, its touch mapping, and
  the AYN key toggle (legacy d152fb370).
- **USB unplug.** Legacy once saw an unplug reset the USB controller and
  drop DRM master, leaving both panels black with
  `Atomic commit failed: Permission denied` (legacy work/task-001, task-002).
  The first boot is tethered by USB; note whether this recurs.

## Write and boot (owner)

The image is an aarch64 store path that any host can hold. Write it to a
microSD card only, then select BOOT MODE = Loader in the bootloader menu.
Check the target device node before writing; never write `sda*` on the Thor.
On the installed Thor, do not insert such a card: its labels collide with
the internal system (see the label collision above).
