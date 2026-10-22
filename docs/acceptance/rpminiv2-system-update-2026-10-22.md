# Mini V2 internal-UFS system update — 2026-10-22

## Result — accepted and left running on the new system

| Item | Final result |
|---|---|
| Main source | `4e34b8a9ed859560acac59c218c7af872221d3ed` |
| Booted/profile system | `hp7kgka5q3abwy25n48p0a511j6kxqjl`, generation 29 |
| Fallback system | Original `7yvsvmm3nszi2lrv3ry6rybk7c9swxs8`, generation 24 plus previous profile |
| Active/fallback service bundles | `z3sypf14liws870wfhrc8w334iicr363` / `7xbkg2lhdsa26v01n6zi8jmqwrv3hr9q` |
| Services, trust, limits/uclamp, real launch | Passed. No failed units. |
| Bubblewrap | Not implemented in core at `4e34b8a9e`; parent corrected and waived that gate. |
| Rollback | Exercised twice live; exact prior GRUB bytes and old system/bundle recovered. |
| Space cleanup | Reclaimed 8.05 GiB; root free 3.17 → 11.22 GiB. Protected payload/save/identity/trust preserved. |

## Scope and source

The owner approved one Mini V2 update. The parent must release the device from
`bench` before any write. No other device, block write, partition-table change,
firmware change or backup of user data is part of this operation. Only the
on-device GRUB/profile rollback is retained.

Fetched and rebased onto `origin/main` at
`4e34b8a9ed859560acac59c218c7af872221d3ed`. A later read-only `ls-remote`
confirmed the same main tip. The factoring does not change the system path:

```text
/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
```

## Offline verification

| Check | Result |
|---|---|
| `nix run .#rpminiv2-update-check` | 8 passed; production renderer for two toplevels, new/previous order, unsafe config and mount/label refusals, low-space refusal, existing-payload protection, real filesystem and Nix-profile rollback. ShellCheck passed. |
| `nix build --no-link .#checks.x86_64-linux.rpminiv2` | Passed: 34 image-verifier tests, 2 fan-map tests, 8 kernel-config tests and 2 thermal tests, plus module assertions. |
| System + shared firmware builds | Passed on Zao/configured builders; no target build. |
| Ruff check/format; Python bytecode compilation | Passed for the new Python files. No project Python typechecker is configured. |
| Nix evaluation + formatting | New Nix expressions formatted; product/recovery module evaluation passed. |
| `nix run .#help` | Both update apps listed. |
| `nix run .#rpminiv2-update -- --status` | Real read-only path passed on Mini V2 SSH port 2222. |

### Image firmware identity

Before editing, saved the evaluated original `sdImage.populateFirmwareCommands`
from main. Ran that exact script into `/tmp/rpminiv2-firmware-before.3kjmhT/`
after its referenced paths were built. `diff -r` against the new shared output
`/nix/store/qvyl7xyvrq9gmxblxyifx1jzsdp55cj2-rpminiv2-firmware` exited 0.
All **9 filenames and file bytes** match for the same toplevel, including
GRUB, BLS, kernel, initrd, DTB, font and both loaders. The system toplevel path
also equals the pre-edit evaluated path. Whole disk-image GUIDs are already
non-deterministic; this proof compares the actual populated firmware contents,
not full-image hashes.

```text
39de9119311fa4274f27908a561f2b876133810325d9deff89a9461f832c838b  EFI/BOOT/BOOTAA64.EFI
f9de4f9f3faa4b914901eab6eeb819a2dba79b6398c709bab73df5d3ddd317e9  EFI/nixos/s1yqrmp6xk30x8h7vbywkm73jnwpsmni-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
3692c7ea66cda7228d7755737318365252881e4d76ec9a7d5e0ae9508020aa3e  EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-Image
53abe644510292a5399bed66277e2ef558067789834084d58eeb72b400641543  EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
ed24e7e8b385f25f9df8812c899ecf581a6aa87368827b6a4a41fe4cb6f513cd  EFI/systemd/systemd-bootaa64.efi
734f45a5b8c134b5cc161d02a9650fa7cf939abbab1a32f254bab8da201c9385  boot/grub/dejavu-mono.pf2
505dd4b1ca61f81d2a386dde578f616c13dbba77c3fdb02e8c644903cb63ec42  boot/grub/grub.cfg
b3f4929575ff82b657d9234e3e2bda7f2964baa01b5af7c9f4c292b0399c50c0  loader/entries/nixos-generation-1.conf
a6c7f5aafd644a48e48efb3f953c08f946cbf443f40ed74e108fb6c91dc127af  loader/loader.conf
```

## Device acceptance

Parent gave go and exclusive use after confirming bench/Cemu/transfers/hashing
were stopped. Permission includes GC without `-d` if needed, then deletion of
old system generations only if still needed. The BotW `.wua` and Korri library
must remain untouched. The updater code is commit `29ebeaaee`.

### Before (before any update write)

```text
UTC=2026-10-22T23:39:44+00:00
booted=/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
profile=/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
   1   1970-01-01 00:00:08
   2   2026-10-01 19:18:01
   3   2026-10-02 05:01:20
   4   2026-10-02 14:31:02
   5   2026-10-06 16:44:01
   6   2026-10-07 00:21:55
   7   2026-10-07 00:22:46
   8   2026-10-07 00:25:06
   9   2026-10-15 23:02:57
  10   2026-10-16 00:07:38
  11   2026-10-17 20:10:39
  12   2026-10-18 01:36:25
  13   2026-10-18 04:03:35
  14   2026-10-18 04:06:09
  15   2026-10-18 05:57:50
  16   2026-10-18 22:09:58
  17   2026-10-18 22:51:22
  18   2026-10-18 23:32:42
  19   2026-10-19 07:30:30
  20   2026-10-22 01:48:41
  21   2026-10-22 02:48:59
  22   2026-10-22 08:36:07
  23   2026-10-22 17:06:49
  24   2026-10-22 20:11:12   (current)
TARGET SOURCE     FSTYPE OPTIONS
/boot  /dev/sda21 vfat   rw,relatime,fmask=0022,dmask=0022,codepage=437,iocharset=ascii,shortname=mixed,errors=remount-ro
TARGET SOURCE     FSTYPE OPTIONS
/      /dev/sda22 ext4   rw,relatime,stripe=128
sda21   512M part vfat RPMINIV2       /boot
sda22 116.2G part ext4 NIXOS_RPMINIV2 /nix/store
                                      /
Filesystem        1B-blocks         Used  Available Use% Mounted on
/dev/sda21        535560192    379822080  155738112  71% /boot
/dev/sda22     122244853760 112539557888 3448340480  98% /
ESP FILES (SHA-256):
734f45a5b8c134b5cc161d02a9650fa7cf939abbab1a32f254bab8da201c9385  /boot/boot/grub/dejavu-mono.pf2
5951418c0b65755bee9e665787c83020897338fedf11f8f4c1ea4dbabb142373  /boot/boot/grub/grub.cfg
d35629dfadecb1ec4d33df5241cf8f0f072df81508ff30008695d5a7866fbc79  /boot/boot/grub/grub.cfg.20261007T002155Z.bak
d35629dfadecb1ec4d33df5241cf8f0f072df81508ff30008695d5a7866fbc79  /boot/boot/grub/grub.cfg.20261007T002506Z.bak
db1a9000895c6d37c39b73b6cb29156844be400b7f58a765824f106a6314917e  /boot/boot/grub/grub.cfg.20261017T201039Z.bak
358b2fd08b7c333d58e10ab9b8838f56d041b70cf129a418190943925fb6334f  /boot/boot/grub/grub.cfg.20261018T013625Z.bak
7b31f092549e1fcca7666172efa5a1726a951cf2bb1b4f202d70501e2a3d7a07  /boot/boot/grub/grub.cfg.20261022T014841Z.bak
d7aef92da7db65c0409a95e04165d3d02c76cae89c1b6ee17c9581de19cf1256  /boot/boot/grub/grub.cfg.20261022T024859Z.bak
d280b32a0fec452e079a0ed0cd687f94cf70c03f50695efa157b45bfa70deb5c  /boot/boot/grub/grub.cfg.20261022T083839Z.bak
99a6439127328de529fe18b184b1921ed9d2756859086b07da2742a8ac6c7636  /boot/boot/grub/grub.cfg.20261022T170814Z.bak
31c8e3072922b24a36d719724011a9cde7b9d421840ab9ec02b0a38747d7cbb5  /boot/boot/grub/grub.cfg.20261022T201130Z.bak
39de9119311fa4274f27908a561f2b876133810325d9deff89a9461f832c838b  /boot/EFI/BOOT/BOOTAA64.EFI
746ef802a81a3d5becd6840e4a7d211210d77ca8d8b464056204a5892364c0c1  /boot/EFI/nixos/4lllsfhslww6jx8xxx8309nd8ad0bx49-linux-aarch64-unknown-linux-gnu-7.2-Image
53abe644510292a5399bed66277e2ef558067789834084d58eeb72b400641543  /boot/EFI/nixos/4lllsfhslww6jx8xxx8309nd8ad0bx49-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
8aa28b0c2b6f613e313b1523f33c75441f4008e5f9bc47654c3ea8e5a07c379c  /boot/EFI/nixos/bdilkc51nd59xca6k7bkqnivndc9mivc-linux-aarch64-unknown-linux-gnu-7.2-Image
53abe644510292a5399bed66277e2ef558067789834084d58eeb72b400641543  /boot/EFI/nixos/bdilkc51nd59xca6k7bkqnivndc9mivc-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
3b73acf466fcd3d2e7b7ee64970acd6f2b2a473ff5de526303f65357eff13320  /boot/EFI/nixos/k7iqsb94nnfih2jpl4xpna9b3a923wcd-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
89173229d4a734b2471772813b2048a154d3e47472e06b0a9a702606e26a36ac  /boot/EFI/nixos/r1i7iik6fygsv2z4883v7gx6izgrm695-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
f9de4f9f3faa4b914901eab6eeb819a2dba79b6398c709bab73df5d3ddd317e9  /boot/EFI/nixos/s1yqrmp6xk30x8h7vbywkm73jnwpsmni-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
43b82cd0ad28d1bc0416a3b9d37e3eb049e4b21f2d667b0c36ce54704f8695fb  /boot/EFI/nixos/x3b1msrllg4d0ldzy95yqrmlv7370c2m-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
cfcca9fa284e3f7288642013f6ed519cf2561590aa5cb394a4b9771952469670  /boot/EFI/nixos/y48fgb5fc27lzpxzfq9l41v9abq1pw70-linux-aarch64-unknown-linux-gnu-7.2-Image
53abe644510292a5399bed66277e2ef558067789834084d58eeb72b400641543  /boot/EFI/nixos/y48fgb5fc27lzpxzfq9l41v9abq1pw70-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
3692c7ea66cda7228d7755737318365252881e4d76ec9a7d5e0ae9508020aa3e  /boot/EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-Image
53abe644510292a5399bed66277e2ef558067789834084d58eeb72b400641543  /boot/EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
8ce1b4d2c9738f197c87a9418a31369a11093c5e0f7e78b24c0b268618198c4f  /boot/EFI/systemd/systemd-bootaa64.efi
0b40f24cadbf43ccadf690d836e545525380fd8481fe8b5915fb8dcdc15691bf  /boot/loader/entries/nixos-generation-1.conf
a6c7f5aafd644a48e48efb3f953c08f946cbf443f40ed74e108fb6c91dc127af  /boot/loader/loader.conf
2cbbf65577fbaea06b0d12573e90c8108b742671bbfab7f5aeb69e4d81c61797  /boot/ubootefi.var
PLUGIN TRUST FILES:
996cf8dafc4800f0f22d40a2c58a1f44f2825b50c02ac8eb68396c02c0871d33  /etc/korri-plugin-host/publishers.json
builders =
fallback = false
max-jobs = 0
require-sigs = true
substituters = https://cache.nixos.org/ https://korri-os.github.io/nix-cache/
trusted-public-keys = cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= korri-cache-fuji-1:E8MOww6FoNRlVavEll8JPc2XHYC4HhZnrhqQcd64OtQ= korri-cache-zao-1:thKjQnMnPl8AqTuZWJTn+ej9BORTPqzleGue4ZfJ2u4= korri-cache-ci-1:iH8gsPMtGrreeuXt2kt6M2ca+y4u/dO+T2km6AUuI8c= korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98= simonwjackson-plugins-1:4SchQgTMNU3syZ2ZzN/cJXPMigc/PIt+EaqHAStgJs8=
  UNIT LOAD ACTIVE SUB DESCRIPTION

0 loaded units listed.
active
active
active
active
```


### Space gate before closure copy

Queried all valid paths from the Mini V2 store (`nix path-info --all`)
and compared them with host `nix path-info --recursive --json` for the built
toplevel. No transfer/build was requested by this query.
An initial `nix-store --query --all` probe was unsupported and discarded;
the successful modern CLI query above supplied the inventory.

| Measurement | Bytes / count |
|---|---:|
| System closure paths | 1675 |
| Paths absent on device | 25 |
| Missing paths, sum of uncompressed NAR sizes | 52748992 |
| Root available | 3448340480 |
| Estimated root remaining after copy | 3395591488 |
| Required reserve | 1073741824 |

Space gate passed. No GC or generation deletion was needed.

### Updater execution (before reboot)

Command:

```sh
nix run .#rpminiv2-update -- --expected-system /nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
```

```text
evaluation warning: stdenv.isLinux is deprecated, use stdenv.hostPlatform.isLinux instead
evaluation warning: stdenv.isLinux is deprecated, use stdenv.hostPlatform.isLinux instead
evaluation warning: stdenv.isLinux is deprecated, use stdenv.hostPlatform.isLinux instead
evaluation warning: stdenv.isDarwin is deprecated, use stdenv.hostPlatform.isDarwin instead
evaluation warning: The xorg package set has been deprecated, 'xorg.libX11' has been renamed to 'libx11'
evaluation warning: Scripted initrd is deprecated and scheduled for removal in 26.11. See the NixOS 26.05 release notes.
evaluation warning: The xorg package set has been deprecated, 'xorg.libX11' has been renamed to 'libx11'
evaluation warning: Scripted initrd is deprecated and scheduled for removal in 26.11. See the NixOS 26.05 release notes.
evaluation warning: stdenv.isLinux is deprecated, use stdenv.hostPlatform.isLinux instead
evaluation warning: stdenv.isDarwin is deprecated, use stdenv.hostPlatform.isDarwin instead
evaluation warning: The xorg package set has been deprecated, 'xorg.libX11' has been renamed to 'libx11'
evaluation warning: Scripted initrd is deprecated and scheduled for removal in 26.11. See the NixOS 26.05 release notes.
evaluation warning: The xorg package set has been deprecated, 'xorg.libX11' has been renamed to 'libx11'
evaluation warning: Scripted initrd is deprecated and scheduled for removal in 26.11. See the NixOS 26.05 release notes.
added 109 signatures
copying 25 paths...
copying path '/nix/store/nvs92ijqg09cranbzg1d0f2whybvvr32-korrid-0.0.0' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/1gxa941977w05q5yvs5nj8v2zw91dlaa-system-path' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/qakzk715c03q8biy9rlkpjs77d37scyy-system-path' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/2ws7hsmajm0v1713hnhxjxy7wz9vr1ny-dbus-1' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/yr2jlc4ss5313hdivkj2d7z193vlpyql-X-Restart-Triggers-dbus' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/237rnc0wrkz3m55mb49ch17188cpldmn-unit-dbus.service' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/7fv7m0688slwqpy4rh1kpkq4ayr59ga9-X-Restart-Triggers-polkit' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/52cc5hzflxnmbqfkx2q1pjw3m4q2h8by-unit-polkit.service' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/z3sypf14liws870wfhrc8w334iicr363-korri-bundle-0.0.0' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/aaalsswf1789qvdwacnfv10h8h2xzwxr-unit-korri-bundle-selector.service' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/68v888zg5cs3agi891w8w0x4hn509029-system-units' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/g3vn0xfwgzxq5r7sxdr1mn38qg1w92s0-dbus-1' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/7hi61l2355skf2adfz01zdc7nn6nlly2-X-Restart-Triggers-dbus' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/fyb4mc8sjaghyrkf29nyb6blnn4pq14w-unit-dbus.service' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/vdbkigvqlzllv74rnk8pa0f4x2jyv9km-X-Restart-Triggers-polkit' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/y6xq73m3h1k4xzan8j7yafpw1qqq1kzq-unit-polkit.service' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/pddlflcxv3wbn0rzf2ci9zpk0bg65f8p-system-units' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/krr5hq4blfyz8kd3gr4ghg23c3f32ym6-unit-dbus.service' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/sga3dk5m32mknb2dn7l6nwb8h0hqmvb8-user-units' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/p4lhdyqj3dr67ghnmhbg83izcac8j6a2-etc' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/p7sispvh2mgqsb9f75v29gzwysajp89d-unit-dbus.service' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/qbb1a2jqpl7dmc0wrm8v4zqag7iafkik-user-units' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/wxfcbknq2qly93hi5fvz73lyazzhqkys-etc' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/sh4nl6nbc5vnzi52b10yrn147hf499kr-nixos-system-rpminiv2-development-sd-card-26.11.20260923.4975466' to 'ssh-ng://root@10.42.3.1'...
copying path '/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466' to 'ssh-ng://root@10.42.3.1'...
grub.cfg: OK
payload/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-Image: OK
payload/s1yqrmp6xk30x8h7vbywkm73jnwpsmni-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd: OK
payload/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb: OK
BOOTAA64.EFI: OK
dejavu-mono.pf2: OK
Update staged. Reboot separately, then verify the booted system.
current=/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
profile=/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
grub_sha256=c795ae90baeddc0b9ffe8511d1db63d110e95f27fe248204e9e79d0810efbe75
previous_profile=/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
Filesystem      Size  Used Avail Use% Mounted on
/dev/sda21      511M  363M  149M  71% /boot
/dev/sda22      114G  105G  3.2G  98% /
TARGET SOURCE     FSTYPE OPTIONS
/boot  /dev/sda21 vfat   rw,relatime,fmask=0022,dmask=0022,codepage=437,iocharset=ascii,shortname=mixed,errors=remount-ro
TARGET SOURCE     FSTYPE OPTIONS
/      /dev/sda22 ext4   rw,relatime,stripe=128
New system: /nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
```

```text
  23   2026-10-22 17:06:49
  24   2026-10-22 20:11:12
  25   2026-10-22 23:41:14   (current)
c795ae90baeddc0b9ffe8511d1db63d110e95f27fe248204e9e79d0810efbe75  /boot/boot/grub/grub.cfg
5951418c0b65755bee9e665787c83020897338fedf11f8f4c1ea4dbabb142373  /boot/boot/grub/grub.cfg.previous
/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466	korri-cache-zao-1:bgCf2ryFeFSB57TDxU8k88z1pKHyjIk8lPQD8LlVe1eF1zcE+qCrd4ddKxs297NG9khPh7S5lGomHCZACdioBg==
insmod part_gpt
insmod part_msdos
set timeout=2
set default=0
set timeout_style=menu
set lang=en_US
loadfont /boot/grub/dejavu-mono.pf2
set rotation=270
set gfxmode=auto
insmod efi_gop
insmod gfxterm
terminal_output gfxterm
set menu_color_normal=cyan/blue
set menu_color_highlight=white/blue

menuentry 'NixOS Retroid Pocket Mini V2 Korri' {
        search --set -f /EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-Image
        linux /EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-Image init=/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466/init plymouth.ignore-serial-consoles quiet logo.nologo quiet rootwait console=tty0 consoleblank=60 video=efifb:off gpt systemd.gpt_auto=0 rd.systemd.gpt_auto=0 splash loglevel=0 lsm=landlock,yama,bpf
        initrd /EFI/nixos/s1yqrmp6xk30x8h7vbywkm73jnwpsmni-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
        devicetree /EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
}

menuentry 'NixOS Retroid Pocket Mini V2 (7yvsvmm3) (previous)' {
        search --set -f /EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-Image
        linux /EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-Image init=/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466/init plymouth.ignore-serial-consoles quiet logo.nologo quiet rootwait console=tty0 consoleblank=60 video=efifb:off gpt systemd.gpt_auto=0 rd.systemd.gpt_auto=0 splash loglevel=0 lsm=landlock,yama,bpf
        initrd /EFI/nixos/s1yqrmp6xk30x8h7vbywkm73jnwpsmni-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
        devicetree /EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
}
```

### After reboot

SSH returned 30.9 seconds after the reconnect poll began. The booted system
matched the new toplevel.

```text
UTC=2026-10-22T23:42:37+00:00
booted=/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
profile=/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
fallback-profile=/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
 23:42:37  up   0:00,  2 users,  load average: 0.46, 0.12, 0.04
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
Id=korrid.service
ActiveState=active
SubState=running
MainPID=1900
Result=success

Id=korri-plugin-host.service
ActiveState=active
SubState=exited
MainPID=0
Result=success

Id=korri-compositor.service
ActiveState=active
SubState=running
MainPID=1691
Result=success

Id=korri-chromium-kiosk.service
ActiveState=active
SubState=running
MainPID=1864
Result=success

Id=korri-limits.service
ActiveState=active
SubState=running
MainPID=1016
Result=success
PLUGIN PUBLISHER TRUST=996cf8dafc4800f0f22d40a2c58a1f44f2825b50c02ac8eb68396c02c0871d33  /etc/korri-plugin-host/publishers.json
builders =
fallback = false
max-jobs = 0
require-sigs = true
substituters = https://cache.nixos.org/ https://korri-os.github.io/nix-cache/
trusted-public-keys = cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= korri-cache-fuji-1:E8MOww6FoNRlVavEll8JPc2XHYC4HhZnrhqQcd64OtQ= korri-cache-zao-1:thKjQnMnPl8AqTuZWJTn+ej9BORTPqzleGue4ZfJ2u4= korri-cache-ci-1:iH8gsPMtGrreeuXt2kt6M2ca+y4u/dO+T2km6AUuI8c= korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98= simonwjackson-plugins-1:4SchQgTMNU3syZ2ZzN/cJXPMigc/PIt+EaqHAStgJs8=
CONFIG_UCLAMP_TASK=y
CONFIG_UCLAMP_BUCKETS_COUNT=5
CONFIG_UCLAMP_TASK_GROUP=y
/sys/fs/cgroup/korri.slice
cpu io memory pids
max
max
/sys/fs/cgroup/korri.slice/korri-games.slice
max
max
ExecStart={ path=/nix/store/cfv04952x2dldrfxsbpr9hziq5fwc5lm-korri-limitsd-0.0.0/bin/korri-limitsd ; argv[]=/nix/store/cfv04952x2dldrfxsbpr9hziq5fwc5lm-korri-limitsd-0.0.0/bin/korri-limitsd ; ignore_errors=no ; start_time=[n/a] ; stop_time=[n/a] ; pid=0 ; code=(null) ; status=0/0 }
Environment="LIMITSD_DEFAULTS={\"cpu\":{\"max\":100,\"min\":0},\"governor\":\"schedutil\",\"gpu\":{\"max\":100,\"min\":0}}" LIMITSD_GPU_NODES= LIMITSD_PEER_UID=976 LOCALE_ARCHIVE=/nix/store/k8as1kdn16q5w6zq6z3vih70qby9l40i-glibc-locales-2.42-84/lib/locale/locale-archive PATH=/nix/store/qbxkcbbkharkrdfbhh0wwh2zf733pzzg-util-linux-2.42.3-bin/bin:/nix/store/p7xdvcvl4aw7m229qzlq601aq0w0nb9n-coreutils-9.11/bin:/nix/store/xqx65i7cwacw7rsnj3dlbqas27mwvpn2-findutils-4.11.0/bin:/nix/store/3b3k72bbr01bgff7bdkiz6r05gmlx5s9-gnugrep-3.12/bin:/nix/store/1nwgsy4c5862g44s9lkf50643p3nbwm5-gnused-4.10/bin:/nix/store/x0iv5hlpqfrpykdckbj8jdxy78clbjpz-systemd-261.2/bin:/nix/store/qbxkcbbkharkrdfbhh0wwh2zf733pzzg-util-linux-2.42.3-bin/sbin:/nix/store/p7xdvcvl4aw7m229qzlq601aq0w0nb9n-coreutils-9.11/sbin:/nix/store/xqx65i7cwacw7rsnj3dlbqas27mwvpn2-findutils-4.11.0/sbin:/nix/store/3b3k72bbr01bgff7bdkiz6r05gmlx5s9-gnugrep-3.12/sbin:/nix/store/1nwgsy4c5862g44s9lkf50643p3nbwm5-gnused-4.10/sbin:/nix/store/x0iv5hlpqfrpykdckbj8jdxy78clbjpz-systemd-261.2/sbin TZDIR=/nix/store/i8an849ir6g3f4n2wrsm88s2qgrzjayq-tzdata-2026c/share/zoneinfo
TARGET SOURCE     FSTYPE OPTIONS
/boot  /dev/sda21 vfat   rw,relatime,fmask=0022,dmask=0022,codepage=437,iocharset=ascii,shortname=mixed,errors=remount-ro
TARGET SOURCE     FSTYPE OPTIONS
/      /dev/sda22 ext4   rw,relatime,stripe=128
Filesystem        1B-blocks         Used  Available Use% Mounted on
/dev/sda21        535560192    379826176  155734016  71% /boot
/dev/sda22     122244853760 112614707200 3373191168  98% /
  23   2026-10-22 17:06:49
  24   2026-10-22 20:11:12
  25   2026-10-22 23:41:14   (current)
734f45a5b8c134b5cc161d02a9650fa7cf939abbab1a32f254bab8da201c9385  /boot/boot/grub/dejavu-mono.pf2
c795ae90baeddc0b9ffe8511d1db63d110e95f27fe248204e9e79d0810efbe75  /boot/boot/grub/grub.cfg
d35629dfadecb1ec4d33df5241cf8f0f072df81508ff30008695d5a7866fbc79  /boot/boot/grub/grub.cfg.20261007T002155Z.bak
d35629dfadecb1ec4d33df5241cf8f0f072df81508ff30008695d5a7866fbc79  /boot/boot/grub/grub.cfg.20261007T002506Z.bak
db1a9000895c6d37c39b73b6cb29156844be400b7f58a765824f106a6314917e  /boot/boot/grub/grub.cfg.20261017T201039Z.bak
358b2fd08b7c333d58e10ab9b8838f56d041b70cf129a418190943925fb6334f  /boot/boot/grub/grub.cfg.20261018T013625Z.bak
7b31f092549e1fcca7666172efa5a1726a951cf2bb1b4f202d70501e2a3d7a07  /boot/boot/grub/grub.cfg.20261022T014841Z.bak
d7aef92da7db65c0409a95e04165d3d02c76cae89c1b6ee17c9581de19cf1256  /boot/boot/grub/grub.cfg.20261022T024859Z.bak
d280b32a0fec452e079a0ed0cd687f94cf70c03f50695efa157b45bfa70deb5c  /boot/boot/grub/grub.cfg.20261022T083839Z.bak
99a6439127328de529fe18b184b1921ed9d2756859086b07da2742a8ac6c7636  /boot/boot/grub/grub.cfg.20261022T170814Z.bak
31c8e3072922b24a36d719724011a9cde7b9d421840ab9ec02b0a38747d7cbb5  /boot/boot/grub/grub.cfg.20261022T201130Z.bak
5951418c0b65755bee9e665787c83020897338fedf11f8f4c1ea4dbabb142373  /boot/boot/grub/grub.cfg.previous
39de9119311fa4274f27908a561f2b876133810325d9deff89a9461f832c838b  /boot/EFI/BOOT/BOOTAA64.EFI
746ef802a81a3d5becd6840e4a7d211210d77ca8d8b464056204a5892364c0c1  /boot/EFI/nixos/4lllsfhslww6jx8xxx8309nd8ad0bx49-linux-aarch64-unknown-linux-gnu-7.2-Image
53abe644510292a5399bed66277e2ef558067789834084d58eeb72b400641543  /boot/EFI/nixos/4lllsfhslww6jx8xxx8309nd8ad0bx49-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
8aa28b0c2b6f613e313b1523f33c75441f4008e5f9bc47654c3ea8e5a07c379c  /boot/EFI/nixos/bdilkc51nd59xca6k7bkqnivndc9mivc-linux-aarch64-unknown-linux-gnu-7.2-Image
53abe644510292a5399bed66277e2ef558067789834084d58eeb72b400641543  /boot/EFI/nixos/bdilkc51nd59xca6k7bkqnivndc9mivc-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
3b73acf466fcd3d2e7b7ee64970acd6f2b2a473ff5de526303f65357eff13320  /boot/EFI/nixos/k7iqsb94nnfih2jpl4xpna9b3a923wcd-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
89173229d4a734b2471772813b2048a154d3e47472e06b0a9a702606e26a36ac  /boot/EFI/nixos/r1i7iik6fygsv2z4883v7gx6izgrm695-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
f9de4f9f3faa4b914901eab6eeb819a2dba79b6398c709bab73df5d3ddd317e9  /boot/EFI/nixos/s1yqrmp6xk30x8h7vbywkm73jnwpsmni-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
43b82cd0ad28d1bc0416a3b9d37e3eb049e4b21f2d667b0c36ce54704f8695fb  /boot/EFI/nixos/x3b1msrllg4d0ldzy95yqrmlv7370c2m-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
cfcca9fa284e3f7288642013f6ed519cf2561590aa5cb394a4b9771952469670  /boot/EFI/nixos/y48fgb5fc27lzpxzfq9l41v9abq1pw70-linux-aarch64-unknown-linux-gnu-7.2-Image
53abe644510292a5399bed66277e2ef558067789834084d58eeb72b400641543  /boot/EFI/nixos/y48fgb5fc27lzpxzfq9l41v9abq1pw70-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
3692c7ea66cda7228d7755737318365252881e4d76ec9a7d5e0ae9508020aa3e  /boot/EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-Image
53abe644510292a5399bed66277e2ef558067789834084d58eeb72b400641543  /boot/EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
8ce1b4d2c9738f197c87a9418a31369a11093c5e0f7e78b24c0b268618198c4f  /boot/EFI/systemd/systemd-bootaa64.efi
0b40f24cadbf43ccadf690d836e545525380fd8481fe8b5915fb8dcdc15691bf  /boot/loader/entries/nixos-generation-1.conf
a6c7f5aafd644a48e48efb3f953c08f946cbf443f40ed74e108fb6c91dc127af  /boot/loader/loader.conf
2cbbf65577fbaea06b0d12573e90c8108b742671bbfab7f5aeb69e4d81c61797  /boot/ubootefi.var
State  Recv-Q Send-Q Local Address:Port  Peer Address:PortProcess
LISTEN 0      4096      127.0.0.54:53         0.0.0.0:*    users:(("systemd-resolve",pid=870,fd=25))
LISTEN 0      4096         0.0.0.0:5355       0.0.0.0:*    users:(("systemd-resolve",pid=870,fd=14))
LISTEN 0      4096   127.0.0.53%lo:53         0.0.0.0:*    users:(("systemd-resolve",pid=870,fd=23))
LISTEN 0      128          0.0.0.0:2222       0.0.0.0:*    users:(("sshd",pid=1855,fd=6))
LISTEN 0      511        127.0.0.1:8099       0.0.0.0:*    users:(("nginx",pid=1294,fd=5),("nginx",pid=1209,fd=5))
LISTEN 0      4096            [::]:5355          [::]:*    users:(("systemd-resolve",pid=870,fd=16))
LISTEN 0      128                *:39217            *:*    users:(("korrid",pid=1900,fd=10))
LISTEN 0      128             [::]:2222          [::]:*    users:(("sshd",pid=1855,fd=7))
```

### Acceptance issue and real rollback

The new system booted and all required services, trust and uclamp checks
passed. However, the existing persistent service-bundle selector correctly
kept its old bundle. `/proc/1900/exe` was
`/nix/store/28jh4iv8mqjfh4wpbnj5wcalgv7l2csh-korrid-0.0.0/bin/korrid`,
not this main's `nvs92ijqg09cranbzg1d0f2whybvvr32-korrid-0.0.0`.
Therefore current-main runtime/bubblewrap acceptance was not established.
The bundle module explicitly preserves active selection across system boots
(`services/inputd/deploy/README.md`). No bundle switch was performed yet.

Followed the required failure policy: ran `--rollback --expected-system` with
the new booted path, then rebooted. The real tool restored the exact before
GRUB SHA-256 and old profile. SSH returned with the old booted system:

```text
evaluation warning: stdenv.isLinux is deprecated, use stdenv.hostPlatform.isLinux instead
current=/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
profile=/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
grub_sha256=5951418c0b65755bee9e665787c83020897338fedf11f8f4c1ea4dbabb142373
previous_profile=/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
Filesystem      Size  Used Avail Use% Mounted on
/dev/sda21      511M  363M  149M  71% /boot
/dev/sda22      114G  105G  3.2G  98% /
TARGET SOURCE     FSTYPE OPTIONS
/boot  /dev/sda21 vfat   rw,relatime,fmask=0022,dmask=0022,codepage=437,iocharset=ascii,shortname=mixed,errors=remount-ro
TARGET SOURCE     FSTYPE OPTIONS
/      /dev/sda22 ext4   rw,relatime,stripe=128
/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
5951418c0b65755bee9e665787c83020897338fedf11f8f4c1ea4dbabb142373  /boot/boot/grub/grub.cfg
/nix/store/7xbkg2lhdsa26v01n6zi8jmqwrv3hr9q-korri-bundle-0.0.0
  UNIT LOAD ACTIVE SUB DESCRIPTION

0 loaded units listed.
```

### Coordinated retry and current-main blocker

Parent approved resuming the same update with deliberate native bundle
selection. Re-staged the same system (0 signatures added, 0 paths copied),
rebooted successfully, then selected
`/nix/store/z3sypf14liws870wfhrc8w334iicr363-korri-bundle-0.0.0`
with the existing `korri-bundle-select switch ... /run/current-system/sw/bin/systemctl`.
Its previous root retained `7xbkg2lhdsa26v01n6zi8jmqwrv3hr9q-korri-bundle-0.0.0`.
The running korrid now resolved to this main's
`/nix/store/nvs92ijqg09cranbzg1d0f2whybvvr32-korrid-0.0.0/bin/korrid`.
No failed units.

Called the real capability-bound loopback `/rpc` (capability was read only
on-device and never logged), `app.local-games.routes` and
`app.local-games.launch.selected` for existing PICORACER-2048 game `16305`,
runner `@korri:fake08/fake08`. Launch response:

```json
{"_tag":"app.local-games.launch.selected","outcome":{"_tag":"Ok","payload":{"session":{"gameId":"16305","launchId":"a521fa68a99df1bc3e398e3438f71b96"},"warnings":[]}}}
```

Unit `korri-game-a521fa68a99df1bc3e398e3438f71b96.service` was active.
Observed process chain: korrid plugin-launch PID 2812 → Gamescope PID 2821 →
gamescopereaper PID 2875 → RetroArch PID 2877. Game cgroup uclamp min was
`0.00`, max `max`. RetroArch had `NoNewPrivs: 1`, `Seccomp: 2`, but shared the
host user namespace (`user:[4026531837]`). **No bubblewrap ran.**

Source inspection confirms that `4e34b8a9e` contains ADR 0015's decision but
no general bubblewrap implementation in korrid. Searching
`services/korrid/src` for `bubblewrap|bwrap` finds only the Steam namespace
comment in `host/systemd_unit.rs:1106`; `launcher/plugin_launch.rs` contains
no wrap effect. No runtime setting or plugin override was changed.
This is a source/acceptance blocker, not a failed ESP update.

Stopped the exact launch through `app.session.stop` (response `phase: stopped`),
restored the recorded old bundle with the native selector, ran the updater's
`--rollback` and rebooted. Verified restored old system, profile, bundle,
exact GRUB hash, trust hash and no failed units:

```text
evaluation warning: stdenv.isLinux is deprecated, use stdenv.hostPlatform.isLinux instead
current=/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
profile=/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
grub_sha256=5951418c0b65755bee9e665787c83020897338fedf11f8f4c1ea4dbabb142373
previous_profile=/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
Filesystem      Size  Used Avail Use% Mounted on
/dev/sda21      511M  363M  149M  71% /boot
/dev/sda22      114G  105G  3.2G  98% /
TARGET SOURCE     FSTYPE OPTIONS
/boot  /dev/sda21 vfat   rw,relatime,fmask=0022,dmask=0022,codepage=437,iocharset=ascii,shortname=mixed,errors=remount-ro
TARGET SOURCE     FSTYPE OPTIONS
/      /dev/sda22 ext4   rw,relatime,stripe=128
/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
/nix/store/7xbkg2lhdsa26v01n6zi8jmqwrv3hr9q-korri-bundle-0.0.0
5951418c0b65755bee9e665787c83020897338fedf11f8f4c1ea4dbabb142373  /boot/boot/grub/grub.cfg
996cf8dafc4800f0f22d40a2c58a1f44f2825b50c02ac8eb68396c02c0871d33  /etc/korri-plugin-host/publishers.json
  UNIT LOAD ACTIVE SUB DESCRIPTION

0 loaded units listed.
active
active
active
active
active
```

### Accepted update after correcting the acceptance plan

Parent confirmed that the bubblewrap gate was a plan error and waived it:
**bubblewrap: not implemented in core at 4e34b8a9e (ADR 0015 is a decision only; grep evidence).**
No bubblewrap code or configuration was added. With approval, re-applied the
same update, rebooted, and selected the new Mini-specific bundle. No new paths
were copied or signatures added on either retry.

Ran a second real korrid launch, waited eight seconds, checked the active
runner chain and cgroup, and stopped the exact launch through korrid:

```json
{"_tag": "app.local-games.launch.selected", "outcome": {"_tag": "Ok", "payload": {"session": {"gameId": "16305", "launchId": "75b6ff638e77da11c7f5aaf19f628c50"}, "warnings": []}}}
```

```text
active
     60       2 root     oom_reaper
   2666       1 korrid   korrid
   2792       1 korri    korrid
   2801    2792 korri    gamescope-wl
   2841    2801 korri    gamescopereaper
   2843    2841 korri    ...retroarch-wr
0.00
max
```

```json
{"_tag": "app.session.stop", "outcome": {"_tag": "Ok", "payload": {"phase": "stopped"}}}
```

Final accepted checks before cleanup:

```text
UTC=2026-10-22T23:51:06+00:00
booted=/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
profile=/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
fallback=/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
bundle=/nix/store/z3sypf14liws870wfhrc8w334iicr363-korri-bundle-0.0.0
old-bundle=/nix/store/7xbkg2lhdsa26v01n6zi8jmqwrv3hr9q-korri-bundle-0.0.0
korrid-exe=/nix/store/nvs92ijqg09cranbzg1d0f2whybvvr32-korrid-0.0.0/bin/korrid
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
996cf8dafc4800f0f22d40a2c58a1f44f2825b50c02ac8eb68396c02c0871d33  /etc/korri-plugin-host/publishers.json
c795ae90baeddc0b9ffe8511d1db63d110e95f27fe248204e9e79d0810efbe75  /boot/boot/grub/grub.cfg
5951418c0b65755bee9e665787c83020897338fedf11f8f4c1ea4dbabb142373  /boot/boot/grub/grub.cfg.previous
builders =
fallback = false
max-jobs = 0
require-sigs = true
portal-profile=/nix/store/3brv24x3hsjf02dmw1g44d8y88hm4h58-korri-portal-0.0.0
HTTP portal index SHA-256=e5da7bf6d70816e8d1704ed43a764e1a4f4a792677fd7e410debff5ec67eeaeb  -
selected portal index SHA-256=e5da7bf6d70816e8d1704ed43a764e1a4f4a792677fd7e410debff5ec67eeaeb  /nix/var/nix/profiles/korri-portal/index.html
/sys/fs/cgroup/korri.slice
max
max
/sys/fs/cgroup/korri.slice/korri-games.slice
max
max
  24   2026-10-22 20:11:12
  25   2026-10-22 23:41:14
  26   2026-10-22 23:43:23
  27   2026-10-22 23:45:18
  28   2026-10-22 23:47:42
  29   2026-10-22 23:49:35   (current)
Filesystem        1B-blocks         Used  Available Use% Mounted on
/dev/sda22     122244853760 112582320128 3405578240  98% /
/dev/sda21        535560192    379826176  155734016  71% /boot
```

## Approved space survey and cleanup

The owner separately approved reclaiming old system generations, unreferenced
store paths, old journals and reconstructible caches. No game payloads, saves,
identity/key files, publisher trust or active/fallback roots are cleanup targets.
All commands target this Mini V2 only.

Before deleting anything, ran the requested read-only `du -xh --max-depth=3 /`
sorted survey (6,105 total survey lines), GC-root inventory and `nix path-info -S`
for its 399 unique store targets, old-generation listing, journal size and cache
surveys. Raw host evidence: `/tmp/rpminiv2-space-survey-20261022.txt`,
`/tmp/rpminiv2-root-closure-sizes-20261022.txt` and
`/tmp/rpminiv2-space-detail-20261022.txt`.

Largest allocated trees from the depth-3 root survey:

```text
456M	/var/log
486M	/nix/store/398fqjqkp383m7pyla9nxpi1is5vzywh-source
523M	/nix/store/yflvcnhrns3sy1qb06wwq9q4814jgw04-llvm-21.1.2-lib
533M	/nix/store/3ph33ykfwcwkb9zrx8yw5i3qh35fypms-llvm-21.1.8-lib
539M	/nix/store/jh55hdi1xnbha52s14rk2im867dlba9i-chromium-unwrapped-143.0.7499.169
600M	/var/lib/systemd
662M	/nix/store/s14nnfb15zhjmkr3w1x2fq6dkz0sk01r-chromium-unwrapped-153.0.8010.52
1.1G	/var/tmp/fallout-miniv2-h841mebc
1.5G	/var/lib/korri-private-cache.h6odJOu2
1.6G	/var/tmp
6.0G	/home
6.0G	/home/korri
6.0G	/home/korri/.local
15G	/nix
15G	/nix/store
81G	/var/lib/korri
83G	/var/lib
85G	/var
105G	/
```

Largest GC-root closure sizes (shared dependencies overlap; do not add them):

```text
/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466	 6737983128
/nix/store/ph6pg4f97f5ckbqqwrqk61njj5pdya1i-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466	 6737881832
/nix/store/sqjq8xcvc8bfqyzr6fj9852c062yxsv2-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466	 6737880344
/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466	 6737873336
/nix/store/c1iagbfqq7mbbvxmnk5wqjbnmvq22wv5-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466	 6736002216
/nix/store/k14ipqkkfvf4c25p1a4aa0ncb6zv7fqd-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466	 6734004936
/nix/store/b5bsg6lf9qndly98vwrxa7iwjg23j3ap-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466	 6153157160
/nix/store/krvfjq25jjvyq9ybycnshvx3sia25jr7-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466	 6152409208
/nix/store/qkzdwjcncb1rxricvzjisqzgka856ghs-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466	 6152290120
/nix/store/ja1d8a1qpyi0yxnavwdhhrh3q29q2iv6-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466	 6152289408
```

The 105 GiB is mainly game data, not a giant log: `/var/lib/korri` is 81 GiB
(28 GiB ROMs, 51 GiB user/native data). Steam is 26 GiB, OpenGOAL 19 GiB,
Skate3's native game copy 6 GiB, and Simpsons 4.2 GiB. Kept installed game
assets and Wine prefixes; those are not clearly discardable caches.

Detailed survey and protected-file baseline:

```text
BEFORE CLEANUP (bytes):
Filesystem        1B-blocks         Used  Available Use% Mounted on
/dev/sda22     122244853760 112586846208 3401052160  98% /
/dev/sda21        535560192    379826176  155734016  71% /boot
KORRI LARGE TREES:
32K	/home/korri/.local/share/vulkan
72K	/var/lib/korri/games/nes/roms
76K	/var/lib/korri/games/nes
84K	/var/lib/korri/cemu-bench-staging/probe-01
144K	/var/lib/korri/users/<account>/states
156K	/home/korri/.local/share/skate3/cache
168K	/var/lib/korri/users/<account>/zquest-classic
196K	/home/korri/.local/share/icons/hicolor
200K	/home/korri/.local/share/icons
264K	/var/lib/korri/users/<account>/.config
284K	/var/lib/systemd/catalog
464K	/var/lib/korri/users/<account>/system
636K	/var/lib/korri/users/<account>/smw
692K	/var/lib/korri/users/<account>/zelda3
744K	/home/korri/.local/state/skate3/logs
752K	/home/korri/.local/state/skate3
772K	/home/korri/.local/state
864K	/var/lib/korri/users/<account>/saves
968K	/var/lib/korri/games/pico8
1.8M	/var/lib/korri/users/<account>/ActRaiserRecomp
2.6M	/home/korri/.local/share/skate3/B13E07DFF9AB6772
2.6M	/var/lib/korri/games/snes
2.6M	/var/lib/korri/games/snes/roms
4.2M	/var/lib/korri/users/<account>/drmario64.us
5.3M	/var/lib/korri/users/<account>/.cache
5.5M	/var/lib/korri/games/zelda-classic
5.5M	/var/lib/korri/games/zelda-classic/roms
8.1M	/var/lib/korri/games/gba
12M	/var/lib/korri/users/<account>/.local
15M	/var/lib/korri/cemu-bench-staging
15M	/var/lib/korri/cemu-bench-staging/save
15M	/var/lib/korri/cemu-bench-staging/save/00050000
20M	/var/lib/korri/games/solarus
20M	/var/lib/korri/games/solarus/roms
35M	/var/lib/korri/users/<account>/2ship
37M	/var/lib/korri/games/n64
37M	/var/lib/korri/games/n64/roms
122M	/var/lib/korri/users/<account>/nocturnerecomp
159M	/var/lib/korri/cache/mega-man-maker
159M	/var/lib/korri/cache/mega-man-maker/game
309M	/var/lib/korri/users/<account>/mega-man-maker
543M	/var/lib/korri/users/<account>/fallout-ce
554M	/var/lib/korri/users/<account>/fallout2-ce
599M	/var/lib/systemd/coredump
600M	/var/lib/systemd
1.5G	/var/lib/korri-private-cache.h6odJOu2
1.5G	/var/lib/korri-private-cache.h6odJOu2/cache
1.5G	/var/lib/korri-private-cache.h6odJOu2/cache/nar
2.1G	/var/lib/korri/cache/proton
2.1G	/var/lib/korri/cache/proton/cachyos
2.3G	/var/lib/korri/cache
4.2G	/var/lib/korri/users/<account>/simpsons
6.0G	/home/korri/.local
6.0G	/home/korri/.local/share
6.0G	/home/korri/.local/share/skate3
6.0G	/home/korri/.local/share/skate3/game
13G	/var/lib/korri/games/wiiu
15G	/var/lib/korri/games/xbox-360
15G	/var/lib/korri/games/xbox-360/roms
19G	/var/lib/korri/users/<account>/opengoal
26G	/var/lib/korri/users/<account>/steam
28G	/var/lib/korri/games
51G	/var/lib/korri/users
51G	/var/lib/korri/users/<account>
81G	/var/lib/korri
STEAM DETAILS:
360M	/var/lib/korri/users/<account>/steam/steamapps/compatdata/0
388M	/var/lib/korri/users/<account>/steam/steamapps/compatdata/274190
388M	/var/lib/korri/users/<account>/steam/ubuntu12_64
395M	/var/lib/korri/users/<account>/steam/steamapps/common/Kingdom New Lands
400M	/var/lib/korri/users/<account>/steam/steamapps/common/Exit the Gungeon
407M	/var/lib/korri/users/<account>/steam/steamapps/compatdata/2379780
411M	/var/lib/korri/users/<account>/steam/steamapps/compatdata/401710
517M	/var/lib/korri/users/<account>/steam/ubuntu12_32
525M	/var/lib/korri/users/<account>/steam/steamapps/common/Broforce
528M	/var/lib/korri/users/<account>/steam/steamapps/common/Into the Breach
566M	/var/lib/korri/users/<account>/steam/steamrt64
578M	/var/lib/korri/users/<account>/steam/package/tmp
594M	/var/lib/korri/users/<account>/steam/steamapps/common/GoNNER
648M	/var/lib/korri/users/<account>/steam/steamrtarm64
797M	/var/lib/korri/users/<account>/steam/steamapps/common/Wizard of Legend
1.2G	/var/lib/korri/users/<account>/steam/appcache
1.2G	/var/lib/korri/users/<account>/steam/appcache/librarycache
1.3G	/var/lib/korri/users/<account>/steam/package
1.6G	/var/lib/korri/users/<account>/steam/steamapps/common/Atomicrops
2.4G	/var/lib/korri/users/<account>/steam/steamapps/common/Crypt of the NecroDancer
4.1G	/var/lib/korri/users/<account>/steam/steamapps/common/Portal
6.8G	/var/lib/korri/users/<account>/steam/steamapps/compatdata
14G	/var/lib/korri/users/<account>/steam/steamapps/common
21G	/var/lib/korri/users/<account>/steam/steamapps
26G	/var/lib/korri/users/<account>/steam
PROTECTED WUA METADATA:
66310:524374 13660330872 1792712141 /var/lib/korri/games/wiiu/The Legend of Zelda - Breath of the Wild (USA) (DLC) (v208).wua
PROTECTED STAGED SAVE TREE SHA-256:
33b3b22f1e7a7f8f32c47c7b49372003420b63101ff8770e3d4717a3896d6338  -
IDENTITY/TRUST FILE SHA-256:
127e6c9f48b9e4d884460b82900aebf317a4e253fea2f003c8781c50c37a1cb5  /var/lib/korrid/identity/device.key
996cf8dafc4800f0f22d40a2c58a1f44f2825b50c02ac8eb68396c02c0871d33  /etc/korri-plugin-host/publishers.json
SELECTORS:
/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
/nix/store/z3sypf14liws870wfhrc8w334iicr363-korri-bundle-0.0.0
/nix/store/7xbkg2lhdsa26v01n6zi8jmqwrv3hr9q-korri-bundle-0.0.0
CANDIDATE CACHE SIZES (allocated bytes):
1574727680	/var/lib/korri-private-cache.h6odJOu2/cache/nar
2365812736	/var/lib/korri/cache
3801088	/root/.cache
16224256	/home/korri/.cache
5459968	/var/lib/korri/users/<account>/.cache
627998720	/var/lib/systemd/coredump
CANDIDATE PRIVATE CACHE CONTENT EXAMPLES:
1c1xv5hw3qby6s6c3770pf9bj3a7297bagvpir05mdimm91szng3.nar.zst
0bwxpisbgvk00qz7lv67yisvxr9jhk0m2fg7nbjdns7fs5aslv7c.nar.zst
0aa1m3lkkf0h5cf6aksxs91wqj7h3gnj2q1jdyb00imxdmi2awyh.nar.zst
0zkkymjpjwrbsr87666m0ivqbr9gw4dx11azj5nr3530i5yl1zqs.nar.zst
0dxsvh0wkin5g7i3n31vr6s05mahkj5xz2d69pmp2fiqd7wr1n39.nar.zst
JOURNAL:
Archived and active journals take up 446.4M in the file system.
```

Cleanup plan (guarded by exact current/fallback systems and both bundle roots,
and refusal while a game/Steam is active):

- GC without `-d`, then delete only system generations 1–23 and 25–28. Keep
  original generation 24 (old fallback) and generation 29 (new booted system),
  plus the updater's explicit previous-system profile. GC again.
- Delete only contents of the cache directories listed below. Preserve their
  root directories and ownership/modes. This can require downloads or artwork/
  shader regeneration on future game use. Do not delete Steam games or prefixes.
- Remove old core-dump files and vacuum archived journals to 64 MiB. Current
  active journal files can leave the final journal total above that bound.
- Keep all plugin selection roots, explicit historical/manual roots, game
  staging directories and the BotW staged save. No automatic library pruning.

```text
/var/lib/korri-private-cache.h6odJOu2/cache/nar
/var/lib/korri/cache
/root/.cache
/home/korri/.cache
/var/lib/korri/users/<account>/.cache
/var/lib/korri/users/<account>/steam/appcache/librarycache
/var/lib/korri/users/<account>/steam/package/tmp
/var/lib/systemd/coredump (regular core files only)
```

### Cleanup results and final state

Complete deletion/GC/journal log: [cleanup manifest](rpminiv2-system-update-2026-10-22-cleanup.txt).
This lists every deleted store path and core-dump file, removed generation
numbers, cache roots/sizes, and journal files.

| Reclaimed item | Result |
|---|---|
| Initial GC (no `-d`) | 2 unreferenced paths, 2.0 MiB; included an unrooted `korri-cemu-bench` helper and `vulkan-tools`. No game or staged-save files were removed. |
| Old system generations | Removed 1–23 and 25–28. Kept 24 (original old system), 29 (new booted system), and previous-system profile. |
| GC after generation removal | 613 paths, 1.2 GiB. No active/fallback closure path was removed. |
| Private downloaded NAR cache | 1,574,727,680 allocated bytes of cache contents removed. |
| Korri reconstructible cache (Proton and Mega Man Maker) | 2,365,812,736 allocated bytes removed. |
| Root/runtime/account `.cache` contents | 3,801,088 + 16,224,256 + 5,459,968 allocated bytes removed. |
| Steam artwork cache and package temp | 1,252,872,192 + 605,372,416 allocated bytes removed. Games, prefixes and account data kept. |
| Old core dumps | 627,998,720 allocated bytes removed. |
| Archived journals | Vacuum reported 415.4 MiB freed; journal total fell from 446.4 MiB to 31 MiB. |

| Filesystem | Free before cleanup | Free after cleanup |
|---|---:|---:|
| Root | 3,401,052,160 bytes (3.17 GiB) | 12,045,590,528 bytes (11.22 GiB) |
| ESP | 155,734,016 bytes | 155,734,016 bytes |

**Root reclaimed 8,644,538,368 bytes (8.05 GiB).** Root use fell from 98% to
90%. This deliberately did not uninstall games or clear old plugin/manual GC
roots. Those roots can retain other historical store outputs.

Protected BotW `.wua` inode/size/mtime are unchanged (13,660,330,872 bytes).
The staged BotW save tree SHA-256, device identity-key SHA-256 and plugin trust
SHA-256 are byte-for-byte unchanged. The current/fallback system and bundle
closures all pass `nix-store --check-validity` after GC. No failed units.

```text
FINAL PROTECTED WUA METADATA:
66310:524374 13660330872 1792712141 /var/lib/korri/games/wiiu/The Legend of Zelda - Breath of the Wild (USA) (DLC) (v208).wua
FINAL PROTECTED STAGED SAVE TREE SHA-256:
33b3b22f1e7a7f8f32c47c7b49372003420b63101ff8770e3d4717a3896d6338  -
FINAL IDENTITY/TRUST SHA-256:
127e6c9f48b9e4d884460b82900aebf317a4e253fea2f003c8781c50c37a1cb5  /var/lib/korrid/identity/device.key
996cf8dafc4800f0f22d40a2c58a1f44f2825b50c02ac8eb68396c02c0871d33  /etc/korri-plugin-host/publishers.json
FINAL SELECTORS:
/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
/nix/store/7yvsvmm3nszi2lrv3ry6rybk7c9swxs8-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
/nix/store/z3sypf14liws870wfhrc8w334iicr363-korri-bundle-0.0.0
/nix/store/7xbkg2lhdsa26v01n6zi8jmqwrv3hr9q-korri-bundle-0.0.0
CURRENT/FALLBACK CLOSURE VALIDITY:
all four closure roots valid
FINAL SERVICES:
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
  24   2026-10-22 20:11:12
  29   2026-10-22 23:49:35   (current)
builders =
fallback = false
max-jobs = 0
require-sigs = true
c795ae90baeddc0b9ffe8511d1db63d110e95f27fe248204e9e79d0810efbe75  /boot/boot/grub/grub.cfg
5951418c0b65755bee9e665787c83020897338fedf11f8f4c1ea4dbabb142373  /boot/boot/grub/grub.cfg.previous
Filesystem        1B-blocks         Used   Available Use% Mounted on
/dev/sda22     122244853760 103950598144 12037300224  90% /
/dev/sda21        535560192    379826176   155734016  71% /boot
```
