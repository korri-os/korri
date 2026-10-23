# Odin 2 Portal system update — 2026-10-22

## Result — accepted

| Item | Before | After |
|---|---|---|
| System generation | 7: `bv325b0lc8jsjv6r85hgd73mblg2ll4h` | 8: `xqbqdwrn3ws25z9dpnhb3nwl47366m8q` |
| Service bundle | `lwsp0ys0vyyb2ygn0dbm3d4nbcvbw0nh` | `983r04y6kbmmygj6gwqi3f9hbniqzyaz` |
| Retained ESP generations | 5, 6, 7 | 6, 7, 8 (9 entries including specialisations) |
| Trust and identity | Recorded hashes | Unchanged |
| loader_a firmware | `dd26994c...d72389a` | Exact same full SHA-256 |
| Services, portal, limits/uclamp | Healthy | Healthy; no failed units |
| Real launch | — | `wl4` through korrid → Gamescope → gamescopereaper → RetroArch/mGBA; stopped through korrid |

Old system and bundle GC roots are retained. The new core source is the Mini's
same main `4e34b8a9ed859560acac59c218c7af872221d3ed`. No rollback was needed.

## Approval and scope

The initial SD-only task was refused at preflight because the live system uses
internal UFS. See [read-only preflight](odin2portal-update-preflight-2026-10-22.md).
The owner then explicitly approved this one update's **ordinary file writes**
on `/dev/sda18` (`ODIN2P_ESP`) and `/dev/sda19` (`ODIN2P_ROOT`). The parent gave
bench-idle go. This is an individually reviewed manual operation, not a new
automated internal-update tool. No GPT, partition, raw-block, firmware, EFI
variable or bootloader-lock write is authorized. No other device is touched.
The owner requires the existing SD boot route to remain available.

Core source: the same `4e34b8a9ed859560acac59c218c7af872221d3ed` used on the
Mini. Host build on Zao dispatched to fuji and completed. A final evaluation of the
local Git flake pinned with `?rev=4e34b8a9ed859560acac59c218c7af872221d3ed`
returned the exact same candidate path, so the Mini-only engineering changes
and this documentation did not alter the Odin system. Candidate:

```text
/nix/store/xqbqdwrn3ws25z9dpnhb3nwl47366m8q-nixos-system-odin2portal-sd-card-26.11.20260923.4975466
/nix/store/983r04y6kbmmygj6gwqi3f9hbniqzyaz-korri-bundle-0.0.0
```

## Loader and SD-route evidence before any write

Observed `bootctl status`: UEFI by Das U-Boot, systemd-boot 261.2,
`BootCurrent=Boot0001` (`scsi 0`), `/EFI/BOOT/BOOTAA64.EFI` on PARTUUID
`16dd55a8-df9f-44a4-871c-5646430798e8` → `/dev/sda18`. The current BLS entry
selects generation 7, `bv325b0...`; stage 1 mounts `/dev/sda19`.
Linux has an SDHCI `mmc0` host at `8804000.mmc`, but detects no inserted SD.
Physical SD insertion/boot was not tested during this operation.

Read `loader_a` (`/dev/sde79`) **without writing it**. Processed its 10 MiB
Android boot-image contents only in host memory: its kernel begins with
gzip-compressed `u-boot-nodtb.bin` and has an appended DTB. The embedded
firmware includes the MMC boot-device driver, MMC rescan/device/list commands,
and the EFI boot manager. The raw appended DTB contains `mmc@8804000`.
This is direct firmware evidence for the existing SD route, not a claim that a
particular rescue SD was boot-tested. The repository `sd-image.nix` comments
document the same U-Boot EFI removable-media SD handoff.

```text
loader_a SHA-256 dd26994c5a8bb354af590831564df4b74de5758fae41475215d72ecd8d72389a
Android boot-image kernel starts with gzip-compressed u-boot-nodtb.bin; extracted only in host memory (trailing DTB retained, not interpreted).
bootefi bootmgr [fdt address]
EFI bootmgr flow
preboot=scsi scan; usb start;
do_boot=bootefi bootmgr
bootcmd=run do_boot
Couldn't find part #%d on mmc device #%d
cmd/bootefi.c
bootefi bootmgr
efi_bootmgr_load() returned: %ld
no mmc device at slot %x
/ubootefi.var
efidebug test bootmgr
  - run simple bootmgr for test
MMC Device %d not found
U-Boot 2025.01-rc2-00231-g5698f9a35201
mmc_bootdev
mmc_bootdev: %s
lib/efi_loader/efi_bootmgr.c
u-boot,efi-bootmgr
bootefi
mmc rescan [mode]
mmc part - lists available partition on current mmc device
mmc dev [dev] [part] [mode] - show or set current mmc device [partition] and set mode
mmc list - lists available devices
U-Boot 2025.01-rc2-00231-g5698f9a35201 (Feb 24 2025 - 17:42:05 +0800)
```

Read-only EFI boot-option decoding showed `Boot0001` through `Boot0008` are
`scsi 0` through `scsi 7`; no SD is currently enumerated. U-Boot can discover
MMC through its existing driver when a card is inserted. This update preserves
that firmware, its boot-manager environment and EFI variable contents; it only
changes files on the existing UFS filesystems. A later rescue card must use
labels different from the internal labels, with matching root selection, to
avoid duplicate-label ambiguity.

The generated candidate builder is
`/nix/store/846imyqydjphdg628ylby8qnp1qlfjhk-systemd-boot/bin/systemd-boot`.
Source inspection confirms `CAN_TOUCH_EFI_VARIABLES = false`. Its normal update
passes Varlink `operation: update`, `touchVariables: false`, `graceful: true`.
The install branch's CLI equivalent is `--variables=no`, but that branch is
**not used**: no `--install-bootloader` or `NIXOS_INSTALL_BOOTLOADER=1`.
No writable firmware-variable operation or firmware updater is invoked.

## Before

```text
UTC=2026-10-23T00:05:52+00:00
board=AYN Odin 2 Portal
booted=/nix/store/bv325b0lc8jsjv6r85hgd73mblg2ll4h-nixos-system-odin2portal-sd-card-26.11.20260923.4975466
profile=/nix/store/bv325b0lc8jsjv6r85hgd73mblg2ll4h-nixos-system-odin2portal-sd-card-26.11.20260923.4975466
bundle=/nix/store/lwsp0ys0vyyb2ygn0dbm3d4nbcvbw0nh-korri-bundle-0.0.0
TARGET SOURCE     FSTYPE OPTIONS
/boot  /dev/sda18 vfat   rw,relatime,fmask=0022,dmask=0022,codepage=437,iocharset=ascii,shortname=mixed,errors=remount-ro
TARGET SOURCE     FSTYPE OPTIONS
/      /dev/sda19 ext4   rw,relatime
sda-host=/sys/devices/platform/soc@0/1d84000.ufshc/host0/target0:0:0/0:0:0:0
ODIN2P_ESP
ODIN2P_ROOT
Filesystem        1B-blocks        Used   Available Use% Mounted on
/dev/sda19     103552118784 28129169408 70115479552  29% /
/dev/sda18       1071509504   106893312   964616192  10% /boot
   1   2026-03-17 19:56:02
   2   2026-10-22 01:22:08
   3   2026-10-22 01:57:30
   4   2026-10-22 02:42:37
   5   2026-10-22 08:31:02
   6   2026-10-22 17:07:13
   7   2026-10-22 20:11:35   (current)
ESP FILE HASHES:
ed24e7e8b385f25f9df8812c899ecf581a6aa87368827b6a4a41fe4cb6f513cd  /boot/EFI/BOOT/BOOTAA64.EFI
9300a3c45f42fcf88a0581143f87e68c8ba19a48ca1d5b24ad010ebc18e306e7  /boot/EFI/nixos/8abwkff2n5bjvxvpibypn7s4cpfqhk6r-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd.efi
58b43b32a12cd67aa9b4f9987266bf5935ef0188d932ed214c19ca3711560343  /boot/EFI/nixos/fazfjynjb3mai38nasing6zgphb2309s-initrd-linux-aarch64-unknown-linux-gnu-7.0.2-initrd.efi
99676720de8a81b2347f92ab44fe2ebd5597c3abe1940584a25503417da49d6a  /boot/EFI/nixos/k7mlpdc9g3n82qnsybvmhcc3vjyb1b6a-linux-aarch64-unknown-linux-gnu-7.0.2-Image.efi
74ba036081882280dae2110e0ebfd9260662d8a6c7679f83a494b2e6df9551ba  /boot/EFI/nixos/k7mlpdc9g3n82qnsybvmhcc3vjyb1b6a-linux-aarch64-unknown-linux-gnu-7.0.2-qcs8550-ayn-odin2portal.dtb.efi
89afe568cc766f01af3d27f113ecef0445d077d976140f92b37ae950f7630a08  /boot/EFI/nixos/m4h6zi1cnn99lnwkl7bl80psd1wmqaws-linux-aarch64-unknown-linux-gnu-7.2-Image.efi
57aedc7331a1dbcb53e44f37eebb518838cbc04c40607e6181c63a9d88054b9b  /boot/EFI/nixos/m4h6zi1cnn99lnwkl7bl80psd1wmqaws-linux-aarch64-unknown-linux-gnu-7.2-qcs8550-ayn-odin2portal.dtb.efi
6a61d9d5e7ded6a86c87f21b7db1b9c30420954430827aff9b5d67a745ed935b  /boot/EFI/nixos/vzgnmzy3n82x69n6ysyxri855xfymb76-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd.efi
ed24e7e8b385f25f9df8812c899ecf581a6aa87368827b6a4a41fe4cb6f513cd  /boot/EFI/systemd/systemd-bootaa64.efi
2a59b2108e001cba0cb4ad9770dc50f3fd17d2d8b3f779c3c126d2fbeab2a982  /boot/loader/entries/nixos-2a59b2108e001cba0cb4ad9770dc50f3fd17d2d8b3f779c3c126d2fbeab2a982.conf
2ed51342ffe05c6ba3ee3c8fa5f1020061589f351e16ad94466914db61cacb59  /boot/loader/entries/nixos-2ed51342ffe05c6ba3ee3c8fa5f1020061589f351e16ad94466914db61cacb59.conf
318f6066261b54a87426c147381be34d39c437bc737e2f19f74d946870b76b13  /boot/loader/entries/nixos-318f6066261b54a87426c147381be34d39c437bc737e2f19f74d946870b76b13.conf
47510337292c3a798c7ba33e26a847abb00e1d8782000c01d59b57cc4cc31ea9  /boot/loader/entries/nixos-47510337292c3a798c7ba33e26a847abb00e1d8782000c01d59b57cc4cc31ea9.conf
47e6cdffab9860cd852e0c9389eda2915a93ad1452ba65610364e48a8dd45400  /boot/loader/entries/nixos-47e6cdffab9860cd852e0c9389eda2915a93ad1452ba65610364e48a8dd45400.conf
5fc88e3f40187a5149785fab94d375d84b307efacb420c6919c3bc19c624a8f3  /boot/loader/entries/nixos-5fc88e3f40187a5149785fab94d375d84b307efacb420c6919c3bc19c624a8f3.conf
6dcdbed04ca1244a190cb209fad937e3e3ed7e3653491dcc19de2719e445191f  /boot/loader/entries/nixos-6dcdbed04ca1244a190cb209fad937e3e3ed7e3653491dcc19de2719e445191f.conf
f0086ce3fb8a8040d61a9673e3c7e51162c783bc5caf2e5352a223d957cebdef  /boot/loader/entries/nixos-f0086ce3fb8a8040d61a9673e3c7e51162c783bc5caf2e5352a223d957cebdef.conf
f9caaab07890dec2bcb90a4d68cc5b7573fa91cc4cd8fa4c813d9a6fa8d05f16  /boot/loader/entries/nixos-f9caaab07890dec2bcb90a4d68cc5b7573fa91cc4cd8fa4c813d9a6fa8d05f16.conf
067fc514414e89fa29898b3d61ce88577bebc2fbcad0ba42c2b769698c8baafc  /boot/loader/entries.srel
25e669a4df430ee05f2a9cf45f7f6f68bae9d36b7e614e450864d41fbb78ad01  /boot/loader/loader.conf
79d22d685b1cfd82173a0fb23f0b6152322e4e1b95e331134a1922d14e8180b9  /boot/loader/random-seed
1558d4d70c58b70a78c83c35cb91e6d552572ab64f83cb21fb4d4c8ca09ba71c  /boot/ubootefi.var
ENTRY GENERATION/INIT LINES:
/boot/loader/entries/nixos-2a59b2108e001cba0cb4ad9770dc50f3fd17d2d8b3f779c3c126d2fbeab2a982.conf:title NixOS
/boot/loader/entries/nixos-2a59b2108e001cba0cb4ad9770dc50f3fd17d2d8b3f779c3c126d2fbeab2a982.conf:version Generation 5 NixOS Zokor sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-22
/boot/loader/entries/nixos-2a59b2108e001cba0cb4ad9770dc50f3fd17d2d8b3f779c3c126d2fbeab2a982.conf:options init=/nix/store/ybm1fryv54cffb7v9lqshj82gn2yj6cg-nixos-system-odin2portal-sd-card-26.11.20260923.4975466/init plymouth.ignore-serial-consoles quiet logo.nologo allow_mismatched_32bit_el0 fw_devlink.strict=1 pcie_ports=compat irqaffinity=0-2 cgroup.memory=nokmem,nosocket nosoftlockup usbcore.interrupt_interval_override=045e:028e:2 console=tty0 console=ttyMSM0,115200n8 splash loglevel=0 lsm=landlock,yama,bpf
/boot/loader/entries/nixos-2ed51342ffe05c6ba3ee3c8fa5f1020061589f351e16ad94466914db61cacb59.conf:title NixOS
/boot/loader/entries/nixos-2ed51342ffe05c6ba3ee3c8fa5f1020061589f351e16ad94466914db61cacb59.conf:version Generation 6 NixOS Zokor sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-22
/boot/loader/entries/nixos-2ed51342ffe05c6ba3ee3c8fa5f1020061589f351e16ad94466914db61cacb59.conf:options init=/nix/store/chyj0802dsr4qq2zf6vk09j9fjp5h3a7-nixos-system-odin2portal-sd-card-26.11.20260923.4975466/init plymouth.ignore-serial-consoles quiet logo.nologo allow_mismatched_32bit_el0 fw_devlink.strict=1 pcie_ports=compat irqaffinity=0-2 cgroup.memory=nokmem,nosocket nosoftlockup usbcore.interrupt_interval_override=045e:028e:2 console=tty0 console=ttyMSM0,115200n8 splash loglevel=0 lsm=landlock,yama,bpf
/boot/loader/entries/nixos-318f6066261b54a87426c147381be34d39c437bc737e2f19f74d946870b76b13.conf:title NixOS (development)
/boot/loader/entries/nixos-318f6066261b54a87426c147381be34d39c437bc737e2f19f74d946870b76b13.conf:version Generation 6 NixOS Zokor development-sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-22
/boot/loader/entries/nixos-318f6066261b54a87426c147381be34d39c437bc737e2f19f74d946870b76b13.conf:options init=/nix/store/l9v9m5ni9hsajhk1zam6j4r9y7fi8gsn-nixos-system-odin2portal-development-sd-card-26.11.20260923.4975466/init allow_mismatched_32bit_el0 fw_devlink.strict=1 pcie_ports=compat irqaffinity=0-2 cgroup.memory=nokmem,nosocket nosoftlockup usbcore.interrupt_interval_override=045e:028e:2 console=tty0 console=ttyMSM0,115200n8 loglevel=7 lsm=landlock,yama,bpf
/boot/loader/entries/nixos-47510337292c3a798c7ba33e26a847abb00e1d8782000c01d59b57cc4cc31ea9.conf:title NixOS
/boot/loader/entries/nixos-47510337292c3a798c7ba33e26a847abb00e1d8782000c01d59b57cc4cc31ea9.conf:version Generation 7 NixOS Zokor sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-22
/boot/loader/entries/nixos-47510337292c3a798c7ba33e26a847abb00e1d8782000c01d59b57cc4cc31ea9.conf:options init=/nix/store/bv325b0lc8jsjv6r85hgd73mblg2ll4h-nixos-system-odin2portal-sd-card-26.11.20260923.4975466/init plymouth.ignore-serial-consoles quiet logo.nologo allow_mismatched_32bit_el0 fw_devlink.strict=1 pcie_ports=compat irqaffinity=0-2 cgroup.memory=nokmem,nosocket nosoftlockup usbcore.interrupt_interval_override=045e:028e:2 console=tty0 console=ttyMSM0,115200n8 splash loglevel=0 lsm=landlock,yama,bpf
/boot/loader/entries/nixos-47e6cdffab9860cd852e0c9389eda2915a93ad1452ba65610364e48a8dd45400.conf:title NixOS (linux-7_0_2-rescue)
/boot/loader/entries/nixos-47e6cdffab9860cd852e0c9389eda2915a93ad1452ba65610364e48a8dd45400.conf:version Generation 5 NixOS Zokor linux-7.0.2-rescue-sd-card-26.11.20260923.4975466 (Linux 7.0.2), built on 2026-10-22
/boot/loader/entries/nixos-47e6cdffab9860cd852e0c9389eda2915a93ad1452ba65610364e48a8dd45400.conf:options init=/nix/store/ssslgaqbnnwzx1knx7fjiigvas84whid-nixos-system-odin2portal-linux-7.0.2-rescue-sd-card-26.11.20260923.4975466/init plymouth.ignore-serial-consoles quiet logo.nologo allow_mismatched_32bit_el0 fw_devlink.strict=1 pcie_ports=compat irqaffinity=0-2 cgroup.memory=nokmem,nosocket nosoftlockup usbcore.interrupt_interval_override=045e:028e:2 console=tty0 console=ttyMSM0,115200n8 splash loglevel=0 lsm=landlock,yama,bpf
/boot/loader/entries/nixos-5fc88e3f40187a5149785fab94d375d84b307efacb420c6919c3bc19c624a8f3.conf:title NixOS (development)
/boot/loader/entries/nixos-5fc88e3f40187a5149785fab94d375d84b307efacb420c6919c3bc19c624a8f3.conf:version Generation 5 NixOS Zokor development-sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-22
/boot/loader/entries/nixos-5fc88e3f40187a5149785fab94d375d84b307efacb420c6919c3bc19c624a8f3.conf:options init=/nix/store/s2kqa6avbcmmas2mhjz6gay13jjxqsmh-nixos-system-odin2portal-development-sd-card-26.11.20260923.4975466/init allow_mismatched_32bit_el0 fw_devlink.strict=1 pcie_ports=compat irqaffinity=0-2 cgroup.memory=nokmem,nosocket nosoftlockup usbcore.interrupt_interval_override=045e:028e:2 console=tty0 console=ttyMSM0,115200n8 loglevel=7 lsm=landlock,yama,bpf
/boot/loader/entries/nixos-6dcdbed04ca1244a190cb209fad937e3e3ed7e3653491dcc19de2719e445191f.conf:title NixOS (linux-7_0_2-rescue)
/boot/loader/entries/nixos-6dcdbed04ca1244a190cb209fad937e3e3ed7e3653491dcc19de2719e445191f.conf:version Generation 7 NixOS Zokor linux-7.0.2-rescue-sd-card-26.11.20260923.4975466 (Linux 7.0.2), built on 2026-10-22
/boot/loader/entries/nixos-6dcdbed04ca1244a190cb209fad937e3e3ed7e3653491dcc19de2719e445191f.conf:options init=/nix/store/30g1cnf24wy8xwkgcv8rk0qcsk43bgyi-nixos-system-odin2portal-linux-7.0.2-rescue-sd-card-26.11.20260923.4975466/init plymouth.ignore-serial-consoles quiet logo.nologo allow_mismatched_32bit_el0 fw_devlink.strict=1 pcie_ports=compat irqaffinity=0-2 cgroup.memory=nokmem,nosocket nosoftlockup usbcore.interrupt_interval_override=045e:028e:2 console=tty0 console=ttyMSM0,115200n8 splash loglevel=0 lsm=landlock,yama,bpf
/boot/loader/entries/nixos-f0086ce3fb8a8040d61a9673e3c7e51162c783bc5caf2e5352a223d957cebdef.conf:title NixOS (linux-7_0_2-rescue)
/boot/loader/entries/nixos-f0086ce3fb8a8040d61a9673e3c7e51162c783bc5caf2e5352a223d957cebdef.conf:version Generation 6 NixOS Zokor linux-7.0.2-rescue-sd-card-26.11.20260923.4975466 (Linux 7.0.2), built on 2026-10-22
/boot/loader/entries/nixos-f0086ce3fb8a8040d61a9673e3c7e51162c783bc5caf2e5352a223d957cebdef.conf:options init=/nix/store/qjn3v2bqv84lg4r53h1smykfkirjgpgj-nixos-system-odin2portal-linux-7.0.2-rescue-sd-card-26.11.20260923.4975466/init plymouth.ignore-serial-consoles quiet logo.nologo allow_mismatched_32bit_el0 fw_devlink.strict=1 pcie_ports=compat irqaffinity=0-2 cgroup.memory=nokmem,nosocket nosoftlockup usbcore.interrupt_interval_override=045e:028e:2 console=tty0 console=ttyMSM0,115200n8 splash loglevel=0 lsm=landlock,yama,bpf
/boot/loader/entries/nixos-f9caaab07890dec2bcb90a4d68cc5b7573fa91cc4cd8fa4c813d9a6fa8d05f16.conf:title NixOS (development)
/boot/loader/entries/nixos-f9caaab07890dec2bcb90a4d68cc5b7573fa91cc4cd8fa4c813d9a6fa8d05f16.conf:version Generation 7 NixOS Zokor development-sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-22
/boot/loader/entries/nixos-f9caaab07890dec2bcb90a4d68cc5b7573fa91cc4cd8fa4c813d9a6fa8d05f16.conf:options init=/nix/store/m8kp1y5gpn28nvkcb7waf6yiiwilw2qz-nixos-system-odin2portal-development-sd-card-26.11.20260923.4975466/init allow_mismatched_32bit_el0 fw_devlink.strict=1 pcie_ports=compat irqaffinity=0-2 cgroup.memory=nokmem,nosocket nosoftlockup usbcore.interrupt_interval_override=045e:028e:2 console=tty0 console=ttyMSM0,115200n8 loglevel=7 lsm=landlock,yama,bpf
EFI variable inventory SHA-256=aac4257ca07418688edb56fb54de106779739c6bf3ef65d7142e2df02b4d1941  -
EFI variable file count=34
PLUGIN TRUST=c4fb42826ffdd99c93c96c7df8818890bc6fc6cfd075af65c6a889754df4acb3  /etc/korri-plugin-host/publishers.json
IDENTITY=6f26058f21e878292c0cb9476b0e10b2fa99e446772ce74520a8b9ec044ac631  /var/lib/korrid/identity/device.key
builders =
fallback = false
max-jobs = 0
require-sigs = true
trusted-public-keys = cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= korri-cache-fuji-1:E8MOww6FoNRlVavEll8JPc2XHYC4HhZnrhqQcd64OtQ= korri-cache-zao-1:thKjQnMnPl8AqTuZWJTn+ej9BORTPqzleGue4ZfJ2u4= korri-cache-ci-1:iH8gsPMtGrreeuXt2kt6M2ca+y4u/dO+T2km6AUuI8c= korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98= simonwjackson-plugins-1:4SchQgTMNU3syZ2ZzN/cJXPMigc/PIt+EaqHAStgJs8=
BEFORE SERVICES:
  UNIT LOAD ACTIVE SUB DESCRIPTION

0 loaded units listed.
active
active
active
active
active
active
```

## Prebuilt delivery

All mount/board/current-system guards passed immediately before delivery.
No SD inference from labels: this run uses the owner-approved UFS scope.

```text
Closure paths: 1533; missing: 27; missing NAR bytes: 52776264; root available: 70115479552; estimated after copy: 70062703288.
```

## Manual boot staging

Signed the closure recursively on Zao with the existing private cache key
(46 signatures added), then `nix copy` transferred 27 prebuilt paths. No key
was printed/copied and no build ran on the device. Rechecked current system,
mount devices and labels before the following individually approved commands:

```sh
nix-env -p /nix/var/nix/profiles/system-odin2portal-previous --set /nix/store/bv325b0lc8jsjv6r85hgd73mblg2ll4h-nixos-system-odin2portal-sd-card-26.11.20260923.4975466
nix-env -p /nix/var/nix/profiles/system --set /nix/store/xqbqdwrn3ws25z9dpnhb3nwl47366m8q-nixos-system-odin2portal-sd-card-26.11.20260923.4975466
env -u NIXOS_INSTALL_BOOTLOADER -u NIXOS_INSTALL_GRUB /nix/store/xqbqdwrn3ws25z9dpnhb3nwl47366m8q-nixos-system-odin2portal-sd-card-26.11.20260923.4975466/bin/switch-to-configuration boot
```

ESP available was 964,616,192 bytes before staging, above the conservative
256 MiB guard. The old system is separately GC-rooted and generation 7 remains.
The active old bundle root is retained until native selection saves it as previous.

```text
Not checking switch inhibitors (action = boot)
Skipping "/boot/EFI/systemd/systemd-bootaa64.efi", same boot loader version in place already.
Skipping "/boot/EFI/BOOT/BOOTAA64.EFI", same boot loader version in place already.
EFI variable inventory SHA-256=aac4257ca07418688edb56fb54de106779739c6bf3ef65d7142e2df02b4d1941  -
1558d4d70c58b70a78c83c35cb91e6d552572ab64f83cb21fb4d4c8ca09ba71c  /boot/ubootefi.var
timeout 3
default nixos-68ef6af65e31345c79d82e36c585dbb5504a95cfff3305e4c8f1ce5be434275e.conf
console-mode keep
   1   2026-03-17 19:56:02
   2   2026-10-22 01:22:08
   3   2026-10-22 01:57:30
   4   2026-10-22 02:42:37
   5   2026-10-22 08:31:02
   6   2026-10-22 17:07:13
   7   2026-10-22 20:11:35
   8   2026-10-23 00:06:53   (current)
```

The full EFI variable file inventory SHA-256 exactly matches the before
inventory. `/boot/ubootefi.var` also matches before. Both systemd-boot EFI
binaries were already current and were skipped. No EFI variable or firmware
write was requested.

## Bundle selection and pre-reboot ESP result

Used this generation's `sw/bin/korri-bundle-select switch` for the Odin-specific
`983r04...` bundle and the existing systemctl. Native health check passed.
Previous `lwsp0y...` bundle remains rooted. Running korrid resolved to the new
`nvs92ijq...` binary; no failed units. This is separate from boot staging.

`configurationLimit=3` now retains generations **6, 7, 8**, each with normal,
development and Linux 7.0.2 rescue entries (9 entries total). Generation 5's
three ESP entries were pruned; system profile generations themselves were not
deleted. The old generation 7 remains bootable. EFI variables and the loader_a
firmware hash remain unchanged before reboot.

```text
ESP AFTER BOOT STAGING:
ed24e7e8b385f25f9df8812c899ecf581a6aa87368827b6a4a41fe4cb6f513cd  /boot/EFI/BOOT/BOOTAA64.EFI
9300a3c45f42fcf88a0581143f87e68c8ba19a48ca1d5b24ad010ebc18e306e7  /boot/EFI/nixos/8abwkff2n5bjvxvpibypn7s4cpfqhk6r-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd.efi
58b43b32a12cd67aa9b4f9987266bf5935ef0188d932ed214c19ca3711560343  /boot/EFI/nixos/fazfjynjb3mai38nasing6zgphb2309s-initrd-linux-aarch64-unknown-linux-gnu-7.0.2-initrd.efi
99676720de8a81b2347f92ab44fe2ebd5597c3abe1940584a25503417da49d6a  /boot/EFI/nixos/k7mlpdc9g3n82qnsybvmhcc3vjyb1b6a-linux-aarch64-unknown-linux-gnu-7.0.2-Image.efi
74ba036081882280dae2110e0ebfd9260662d8a6c7679f83a494b2e6df9551ba  /boot/EFI/nixos/k7mlpdc9g3n82qnsybvmhcc3vjyb1b6a-linux-aarch64-unknown-linux-gnu-7.0.2-qcs8550-ayn-odin2portal.dtb.efi
89afe568cc766f01af3d27f113ecef0445d077d976140f92b37ae950f7630a08  /boot/EFI/nixos/m4h6zi1cnn99lnwkl7bl80psd1wmqaws-linux-aarch64-unknown-linux-gnu-7.2-Image.efi
57aedc7331a1dbcb53e44f37eebb518838cbc04c40607e6181c63a9d88054b9b  /boot/EFI/nixos/m4h6zi1cnn99lnwkl7bl80psd1wmqaws-linux-aarch64-unknown-linux-gnu-7.2-qcs8550-ayn-odin2portal.dtb.efi
6a61d9d5e7ded6a86c87f21b7db1b9c30420954430827aff9b5d67a745ed935b  /boot/EFI/nixos/vzgnmzy3n82x69n6ysyxri855xfymb76-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd.efi
ed24e7e8b385f25f9df8812c899ecf581a6aa87368827b6a4a41fe4cb6f513cd  /boot/EFI/systemd/systemd-bootaa64.efi
0be18eb6e595ab1bb43cdcfe975f93ac5d4cf102a82201c9bb06cd649637e123  /boot/loader/entries/nixos-0be18eb6e595ab1bb43cdcfe975f93ac5d4cf102a82201c9bb06cd649637e123.conf
2ed51342ffe05c6ba3ee3c8fa5f1020061589f351e16ad94466914db61cacb59  /boot/loader/entries/nixos-2ed51342ffe05c6ba3ee3c8fa5f1020061589f351e16ad94466914db61cacb59.conf
318f6066261b54a87426c147381be34d39c437bc737e2f19f74d946870b76b13  /boot/loader/entries/nixos-318f6066261b54a87426c147381be34d39c437bc737e2f19f74d946870b76b13.conf
47510337292c3a798c7ba33e26a847abb00e1d8782000c01d59b57cc4cc31ea9  /boot/loader/entries/nixos-47510337292c3a798c7ba33e26a847abb00e1d8782000c01d59b57cc4cc31ea9.conf
68ef6af65e31345c79d82e36c585dbb5504a95cfff3305e4c8f1ce5be434275e  /boot/loader/entries/nixos-68ef6af65e31345c79d82e36c585dbb5504a95cfff3305e4c8f1ce5be434275e.conf
6dcdbed04ca1244a190cb209fad937e3e3ed7e3653491dcc19de2719e445191f  /boot/loader/entries/nixos-6dcdbed04ca1244a190cb209fad937e3e3ed7e3653491dcc19de2719e445191f.conf
c6a79151061517e2a203c89ce114d077491685d4837b1e3427093933b3c8c027  /boot/loader/entries/nixos-c6a79151061517e2a203c89ce114d077491685d4837b1e3427093933b3c8c027.conf
f0086ce3fb8a8040d61a9673e3c7e51162c783bc5caf2e5352a223d957cebdef  /boot/loader/entries/nixos-f0086ce3fb8a8040d61a9673e3c7e51162c783bc5caf2e5352a223d957cebdef.conf
f9caaab07890dec2bcb90a4d68cc5b7573fa91cc4cd8fa4c813d9a6fa8d05f16  /boot/loader/entries/nixos-f9caaab07890dec2bcb90a4d68cc5b7573fa91cc4cd8fa4c813d9a6fa8d05f16.conf
067fc514414e89fa29898b3d61ce88577bebc2fbcad0ba42c2b769698c8baafc  /boot/loader/entries.srel
5bf9377b54cc55f88645e3d72d167bfee9d1375e4732948f906c73be8eedacf8  /boot/loader/loader.conf
79d22d685b1cfd82173a0fb23f0b6152322e4e1b95e331134a1922d14e8180b9  /boot/loader/random-seed
1558d4d70c58b70a78c83c35cb91e6d552572ab64f83cb21fb4d4c8ca09ba71c  /boot/ubootefi.var
RETAINED ENTRY VERSIONS:
/boot/loader/entries/nixos-0be18eb6e595ab1bb43cdcfe975f93ac5d4cf102a82201c9bb06cd649637e123.conf:version Generation 8 NixOS Zokor development-sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-23
/boot/loader/entries/nixos-2ed51342ffe05c6ba3ee3c8fa5f1020061589f351e16ad94466914db61cacb59.conf:version Generation 6 NixOS Zokor sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-22
/boot/loader/entries/nixos-318f6066261b54a87426c147381be34d39c437bc737e2f19f74d946870b76b13.conf:version Generation 6 NixOS Zokor development-sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-22
/boot/loader/entries/nixos-47510337292c3a798c7ba33e26a847abb00e1d8782000c01d59b57cc4cc31ea9.conf:version Generation 7 NixOS Zokor sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-22
/boot/loader/entries/nixos-68ef6af65e31345c79d82e36c585dbb5504a95cfff3305e4c8f1ce5be434275e.conf:version Generation 8 NixOS Zokor sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-23
/boot/loader/entries/nixos-6dcdbed04ca1244a190cb209fad937e3e3ed7e3653491dcc19de2719e445191f.conf:version Generation 7 NixOS Zokor linux-7.0.2-rescue-sd-card-26.11.20260923.4975466 (Linux 7.0.2), built on 2026-10-22
/boot/loader/entries/nixos-c6a79151061517e2a203c89ce114d077491685d4837b1e3427093933b3c8c027.conf:version Generation 8 NixOS Zokor linux-7.0.2-rescue-sd-card-26.11.20260923.4975466 (Linux 7.0.2), built on 2026-10-23
/boot/loader/entries/nixos-f0086ce3fb8a8040d61a9673e3c7e51162c783bc5caf2e5352a223d957cebdef.conf:version Generation 6 NixOS Zokor linux-7.0.2-rescue-sd-card-26.11.20260923.4975466 (Linux 7.0.2), built on 2026-10-22
/boot/loader/entries/nixos-f9caaab07890dec2bcb90a4d68cc5b7573fa91cc4cd8fa4c813d9a6fa8d05f16.conf:version Generation 7 NixOS Zokor development-sd-card-26.11.20260923.4975466 (Linux 7.2.0), built on 2026-10-22
EFI variable inventory SHA-256=aac4257ca07418688edb56fb54de106779739c6bf3ef65d7142e2df02b4d1941  -
loader_a=dd26994c5a8bb354af590831564df4b74de5758fae41475215d72ecd8d72389a  /dev/disk/by-partlabel/loader_a
active-bundle=/nix/store/983r04y6kbmmygj6gwqi3f9hbniqzyaz-korri-bundle-0.0.0
fallback-bundle=/nix/store/lwsp0ys0vyyb2ygn0dbm3d4nbcvbw0nh-korri-bundle-0.0.0
Filesystem        1B-blocks        Used   Available Use% Mounted on
/dev/sda19     103552118784 28204351488 70040297472  29% /
/dev/sda18       1071509504   106893312   964616192  10% /boot
```

## Boot acceptance

SSH returned 71.6 seconds after reboot. The booted system equals the new
toplevel and generation 8. All required services are active, no failed units,
portal HTTP bytes match the selected profile, publisher trust and device
identity hashes are unchanged. Limits and uclamp are active. The loader_a
firmware SHA-256 is still identical after reboot. All four current/fallback
system/bundle closure roots validate.

```text
UTC=2026-10-23T00:09:31+00:00
booted=/nix/store/xqbqdwrn3ws25z9dpnhb3nwl47366m8q-nixos-system-odin2portal-sd-card-26.11.20260923.4975466
profile=/nix/store/xqbqdwrn3ws25z9dpnhb3nwl47366m8q-nixos-system-odin2portal-sd-card-26.11.20260923.4975466
fallback=/nix/store/bv325b0lc8jsjv6r85hgd73mblg2ll4h-nixos-system-odin2portal-sd-card-26.11.20260923.4975466
bundle=/nix/store/983r04y6kbmmygj6gwqi3f9hbniqzyaz-korri-bundle-0.0.0
previous-bundle=/nix/store/lwsp0ys0vyyb2ygn0dbm3d4nbcvbw0nh-korri-bundle-0.0.0
korrid=/nix/store/nvs92ijqg09cranbzg1d0f2whybvvr32-korrid-0.0.0/bin/korrid
running
  UNIT LOAD ACTIVE SUB DESCRIPTION

0 loaded units listed.
active
active
active
active
active
active
active
active
active
TRUST=c4fb42826ffdd99c93c96c7df8818890bc6fc6cfd075af65c6a889754df4acb3  /etc/korri-plugin-host/publishers.json
IDENTITY=6f26058f21e878292c0cb9476b0e10b2fa99e446772ce74520a8b9ec044ac631  /var/lib/korrid/identity/device.key
builders =
fallback = false
max-jobs = 0
require-sigs = true
trusted-public-keys = cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= korri-cache-fuji-1:E8MOww6FoNRlVavEll8JPc2XHYC4HhZnrhqQcd64OtQ= korri-cache-zao-1:thKjQnMnPl8AqTuZWJTn+ej9BORTPqzleGue4ZfJ2u4= korri-cache-ci-1:iH8gsPMtGrreeuXt2kt6M2ca+y4u/dO+T2km6AUuI8c= korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98= simonwjackson-plugins-1:4SchQgTMNU3syZ2ZzN/cJXPMigc/PIt+EaqHAStgJs8=
/sys/fs/cgroup/korri.slice
cpu io memory pids
max
max
/sys/fs/cgroup/korri.slice/korri-games.slice
max
max
CONFIG_UCLAMP_TASK=y
CONFIG_UCLAMP_BUCKETS_COUNT=5
CONFIG_UCLAMP_TASK_GROUP=y
portal HTTP=47a9d8da2bd59bdc46a231dfac474e383b30d275687a3501dcc4156beaf51257  -
portal selected=47a9d8da2bd59bdc46a231dfac474e383b30d275687a3501dcc4156beaf51257  /nix/var/nix/profiles/korri-portal/index.html
TARGET SOURCE     FSTYPE OPTIONS
/boot  /dev/sda18 vfat   rw,relatime,fmask=0022,dmask=0022,codepage=437,iocharset=ascii,shortname=mixed,errors=remount-ro
TARGET SOURCE     FSTYPE OPTIONS
/      /dev/sda19 ext4   rw,relatime
   1   2026-03-17 19:56:02
   2   2026-10-22 01:22:08
   3   2026-10-22 01:57:30
   4   2026-10-22 02:42:37
   5   2026-10-22 08:31:02
   6   2026-10-22 17:07:13
   7   2026-10-22 20:11:35
   8   2026-10-23 00:06:53   (current)
/nix/store/xqbqdwrn3ws25z9dpnhb3nwl47366m8q-nixos-system-odin2portal-sd-card-26.11.20260923.4975466	fuji-1:QoOGL3DdoApTjOyGSHzv/Q+oOLC1gBltAaSfT5fDVjgCeMcptPWOhTaucJD6yz5CDzLfvyDTESHYffzjO9IVDA== korri-cache-zao-1:6aQBuIWFkstVgHzu/fVsUEp1WKW068EcysNlmJu8UtYhNe2nTPknPWpKT84LxdF03AEEThtD8WP/XlUA3btQCw==
loader_a=dd26994c5a8bb354af590831564df4b74de5758fae41475215d72ecd8d72389a  /dev/disk/by-partlabel/loader_a
All current/fallback system/bundle closures valid
```

## Real korrid launch

The Mini's PICORACER id `16305` is absent on this Odin; a read-only routes
request correctly returned unavailable. Read the Odin's actual catalog and
selected its existing `wl4` route `@korri:mgba/mgba`, using the ordinary
capability-bound loopback `/rpc`. The capability never left the device or
appeared in logs. No plugin installation or setting override was needed.

```json
{"_tag": "app.local-games.launch.selected", "outcome": {"_tag": "Ok", "payload": {"session": {"gameId": "wl4", "launchId": "223b2551fba5dbc4be08c52fb0b62b40"}, "warnings": []}}}
```

After eight seconds, the game service and runner were still active. Actual
chain: korrid plugin-launch → Gamescope → gamescopereaper → RetroArch/mGBA.
Cgroup clamp defaults are 0/max and the limits helper is active. No bubblewrap
check: core at `4e34b8a9e` does not implement ADR 0015's decision.

```text
active
     61       2 root     oom_reaper
   1974       1 korrid   korrid
   2552       1 korri    korrid
   2561    2552 korri    gamescope-wl
   2601    2561 korri    gamescopereaper
   2603    2601 korri    ...retroarch-wr
0.00
max
active
```

Stopped only this exact launch through korrid:

```json
{"_tag": "app.session.stop", "outcome": {"_tag": "Ok", "payload": {"phase": "stopped"}}}
```

Final health:

```text
  UNIT LOAD ACTIVE SUB DESCRIPTION

0 loaded units listed.
active
active
active
active
active
active
active
KORRID=/nix/store/nvs92ijqg09cranbzg1d0f2whybvvr32-korrid-0.0.0/bin/korrid
FINAL FREE:Filesystem        1B-blocks        Used   Available Use% Mounted on
/dev/sda19     103552118784 28219817984 70024830976  29% /
/dev/sda18       1071509504   106893312   964616192  10% /boot
```

Accepted and left running on the new system/bundle. No rollback was needed.
The previous system profile, generation 7 ESP entries and old bundle root are
retained. No cleanup or unrelated change was made on the Odin.

## Final ESP inventory and limits

The complete ESP inventory after reboot is below. It still has 9 BLS entries
for generations 6/7/8. No GRUB configuration or unrelated flashing payload was
present. The pre-existing `ubootefi.var` belongs to U-Boot; the activation did
not modify it. Runtime firmware/bootloader variable/entropy updates during a
normal reboot are distinct from an operator EFI-variable write; the full
inventory was proven unchanged across our staging and bundle operations.
The shared first-boot registration marker is absent; this was not an image
installation or partition-expansion operation.

```text
ESP AFTER REBOOT:
ed24e7e8b385f25f9df8812c899ecf581a6aa87368827b6a4a41fe4cb6f513cd  /boot/EFI/BOOT/BOOTAA64.EFI
9300a3c45f42fcf88a0581143f87e68c8ba19a48ca1d5b24ad010ebc18e306e7  /boot/EFI/nixos/8abwkff2n5bjvxvpibypn7s4cpfqhk6r-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd.efi
58b43b32a12cd67aa9b4f9987266bf5935ef0188d932ed214c19ca3711560343  /boot/EFI/nixos/fazfjynjb3mai38nasing6zgphb2309s-initrd-linux-aarch64-unknown-linux-gnu-7.0.2-initrd.efi
99676720de8a81b2347f92ab44fe2ebd5597c3abe1940584a25503417da49d6a  /boot/EFI/nixos/k7mlpdc9g3n82qnsybvmhcc3vjyb1b6a-linux-aarch64-unknown-linux-gnu-7.0.2-Image.efi
74ba036081882280dae2110e0ebfd9260662d8a6c7679f83a494b2e6df9551ba  /boot/EFI/nixos/k7mlpdc9g3n82qnsybvmhcc3vjyb1b6a-linux-aarch64-unknown-linux-gnu-7.0.2-qcs8550-ayn-odin2portal.dtb.efi
89afe568cc766f01af3d27f113ecef0445d077d976140f92b37ae950f7630a08  /boot/EFI/nixos/m4h6zi1cnn99lnwkl7bl80psd1wmqaws-linux-aarch64-unknown-linux-gnu-7.2-Image.efi
57aedc7331a1dbcb53e44f37eebb518838cbc04c40607e6181c63a9d88054b9b  /boot/EFI/nixos/m4h6zi1cnn99lnwkl7bl80psd1wmqaws-linux-aarch64-unknown-linux-gnu-7.2-qcs8550-ayn-odin2portal.dtb.efi
6a61d9d5e7ded6a86c87f21b7db1b9c30420954430827aff9b5d67a745ed935b  /boot/EFI/nixos/vzgnmzy3n82x69n6ysyxri855xfymb76-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd.efi
ed24e7e8b385f25f9df8812c899ecf581a6aa87368827b6a4a41fe4cb6f513cd  /boot/EFI/systemd/systemd-bootaa64.efi
0be18eb6e595ab1bb43cdcfe975f93ac5d4cf102a82201c9bb06cd649637e123  /boot/loader/entries/nixos-0be18eb6e595ab1bb43cdcfe975f93ac5d4cf102a82201c9bb06cd649637e123.conf
2ed51342ffe05c6ba3ee3c8fa5f1020061589f351e16ad94466914db61cacb59  /boot/loader/entries/nixos-2ed51342ffe05c6ba3ee3c8fa5f1020061589f351e16ad94466914db61cacb59.conf
318f6066261b54a87426c147381be34d39c437bc737e2f19f74d946870b76b13  /boot/loader/entries/nixos-318f6066261b54a87426c147381be34d39c437bc737e2f19f74d946870b76b13.conf
47510337292c3a798c7ba33e26a847abb00e1d8782000c01d59b57cc4cc31ea9  /boot/loader/entries/nixos-47510337292c3a798c7ba33e26a847abb00e1d8782000c01d59b57cc4cc31ea9.conf
68ef6af65e31345c79d82e36c585dbb5504a95cfff3305e4c8f1ce5be434275e  /boot/loader/entries/nixos-68ef6af65e31345c79d82e36c585dbb5504a95cfff3305e4c8f1ce5be434275e.conf
6dcdbed04ca1244a190cb209fad937e3e3ed7e3653491dcc19de2719e445191f  /boot/loader/entries/nixos-6dcdbed04ca1244a190cb209fad937e3e3ed7e3653491dcc19de2719e445191f.conf
c6a79151061517e2a203c89ce114d077491685d4837b1e3427093933b3c8c027  /boot/loader/entries/nixos-c6a79151061517e2a203c89ce114d077491685d4837b1e3427093933b3c8c027.conf
f0086ce3fb8a8040d61a9673e3c7e51162c783bc5caf2e5352a223d957cebdef  /boot/loader/entries/nixos-f0086ce3fb8a8040d61a9673e3c7e51162c783bc5caf2e5352a223d957cebdef.conf
f9caaab07890dec2bcb90a4d68cc5b7573fa91cc4cd8fa4c813d9a6fa8d05f16  /boot/loader/entries/nixos-f9caaab07890dec2bcb90a4d68cc5b7573fa91cc4cd8fa4c813d9a6fa8d05f16.conf
067fc514414e89fa29898b3d61ce88577bebc2fbcad0ba42c2b769698c8baafc  /boot/loader/entries.srel
5bf9377b54cc55f88645e3d72d167bfee9d1375e4732948f906c73be8eedacf8  /boot/loader/loader.conf
71bc271b08eb445fdb0210e35749505d00576469afc596766e23ba9ca71863c5  /boot/loader/random-seed
1558d4d70c58b70a78c83c35cb91e6d552572ab64f83cb21fb4d4c8ca09ba71c  /boot/ubootefi.var
Entry count=9
Current default=default nixos-68ef6af65e31345c79d82e36c585dbb5504a95cfff3305e4c8f1ce5be434275e.conf
POST-REBOOT EFI inventory=9e5a4bf7062940a50da83e35a76d1f2eb8f46ccfbea80915fd8d87392b40dc37  -
Pending first-boot registration=absent
Kernel cmdline=initrd=\EFI\nixos\vzgnmzy3n82x69n6ysyxri855xfymb76-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd.efi init=/nix/store/xqbqdwrn3ws25z9dpnhb3nwl47366m8q-nixos-system-odin2portal-sd-card-26.11.20260923.4975466/init plymouth.ignore-serial-consoles quiet logo.nologo allow_mismatched_32bit_el0 fw_devlink.strict=1 pcie_ports=compat irqaffinity=0-2 cgroup.memory=nokmem,nosocket nosoftlockup usbcore.interrupt_interval_override=045e:028e:2 console=tty0 console=ttyMSM0,115200n8 splash loglevel=0 lsm=landlock,yama,bpf
```

Checks: host toplevel build passed;
`nix build --no-link .#checks.x86_64-linux.odin2portal` passed all module
assertions and its kiosk checks. `git diff --check` passed. This follow-on
changed documentation only, so no new runtime source/test or language
typecheck was needed. The normal staged-secret hook remains enabled.

Unverified: physical SD insertion/boot, physical display/controller quality
and general bubblewrap behavior. The existing firmware and its MMC/EFI boot
capability were preserved; the operation did not change bootloader lock state
or issue any raw block/GPT/firmware/EFI-variable write. The known unlocked-state
physical-access limitations remain; UEFI Secure Boot output is not an Android
lock-state verification. No generic internal-update automation was added.
