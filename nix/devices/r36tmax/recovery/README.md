# R36T Max recovery boot helpers

This packages the `hybrid-boot` procedure. It does not build a NixOS system,
write a card, mount a filesystem, or test the handheld.

## Interface

```nix
recovery = import ./recovery { pkgs = pkgs.buildPackages; };
bootFiles = recovery.bootFiles { system = config.system.build.toplevel; };
```

| Attribute | Contract |
| --- | --- |
| `loader` | Pinned `20260901-a` loader bytes, original `boot.scr`, patched `boot.scr`, and patched `boot.txt` |
| `bootFiles { system; }` | `Image`, `initrd`, patched `boot.scr`, nine identical board DTBs, and `extlinux/extlinux.conf` |
| `tools` | `bin/r36tmax-recovery`; native Python, mtools, mkimage, debugfs, e2label |
| `checks` | Real temporary file/FAT/ext4 tests, including malformed inputs |
| import argument `rocknixArchive` | Optional local archive input; the same pinned SHA256 remains mandatory |

`system` must be the **same derivation** used for the root filesystem and its
module closure. The helper reads `kernel`, `initrd`,
`dtbs/rockchip/rk3326-aislpc-r36t-max.dtb`, `init`, and `kernel-params` from it.
It does not accept kernel, DTB, initrd, or command-line overrides.

This is grounded in `addEntry` in nixpkgs revision
`a6531044f6d0bef691ea18d4d4ce44d0daa6e816`, at
`nixos/modules/system/boot/loader/generic-extlinux-compatible/extlinux-conf-builder.sh`.
In particular, APPEND is `init=<resolved system>/init <kernel-params>`.
The nine DTB aliases, FDTDIR, and `ramdisk_addr_r=0x0c000000` come from the
existing `../hybrid-boot`, not from a new detection scheme. Extraction checks
that the pinned script's complete set of DTB names equals that list.

The optional parent integration is:

```nix
sdImage.firmwareSize = 128;
sdImage.firmwarePartitionOffset = 16;
sdImage.firmwarePartitionName = "NIXOS_BOOT";
sdImage.rootVolumeLabel = "NIXOS_R36TMAX";
sdImage.populateFirmwareCommands = ''
  cp -r ${bootFiles}/. firmware/
  chmod -R u+w firmware
'';
sdImage.postBuildCommands = ''
  dd if=${recovery.loader}/rocknix-loader-full.bin of="$img" \
    bs=512 seek=64 conv=notrunc
  ${recovery.tools}/bin/r36tmax-recovery verify-image \
    ${config.system.build.toplevel} ${recovery.loader} "$img"
'';
```

Keep the existing root population command, with that exact toplevel.
`chmod` permits the upstream image builder's timestamp normalization after
copying read-only store files. `verify-image` runs before image compression.
These are integration snippets, not changes to `sd-image.nix`.

CLI positional arguments:

```text
r36tmax-recovery extract ARCHIVE_GZ NEW_OUTPUT_DIRECTORY
r36tmax-recovery populate SYSTEM LOADER EMPTY_BOOT_DIRECTORY
r36tmax-recovery verify-directory SYSTEM LOADER BOOT_DIRECTORY
r36tmax-recovery verify-fat SYSTEM LOADER FAT_IMAGE
r36tmax-recovery verify-image SYSTEM LOADER RAW_SD_IMAGE
```

Verification compares every FAT payload byte by SHA256 and the complete
extlinux text against the system inputs. It rejects missing or extra boot
files. It requires the FAT label `NIXOS_BOOT` and ext4 label
`NIXOS_R36TMAX`, as declared in `../sd-image.nix`. The image check also
verifies MBR geometry, the raw loader hash, root `init` and `kernel-params`
bytes, and the exact `kernel-modules` symlink.

Module verification compares the supplied system's `lib/modules` subtree,
including its single version directory, every directory entry, every regular
file's SHA256, and every symlink's exact target text. It checks dependency
indexes as well as module files. Empty regular indexes remain valid when the
supplied system has identical empty files. Absolute and relative links resolve
component by component **inside the image**; a target present only on the
host cannot satisfy the check. Linked package subtrees receive the same checks.
This preserves the supplied module closure's provenance, not merely equal
module bytes reached through a different symlink. The layout comes from
nixpkgs' `system.modulesTree` / `aggregateModules` and the `kernel-modules`
link in `nixos/modules/system/boot/kernel.nix`; no new data model is introduced.

This does **not** hash unrelated files in the NixOS closure, validate module
ABI compatibility, or prove that a kernel boots. Module checking reads each
unique path's metadata and every module/index file. The extra debugfs calls
make it slower than boot-only verification.
Use it on a fresh image, not a card containing `flight.log` or other runtime
files. Only regular image files are accepted; mtools and debugfs do not mount
them.

`Image` must be at most 50,331,648 bytes (48 MiB). The existing loader places
it at `0x09000000` and the initrd at `0x0c000000`; one byte above the limit
would overlap the initrd load extent. This check is independent of FAT space.
It does not validate other relocation or decompression limits.

The 1 MiB file-population margin is conservative, not a FAT allocation model.
The real mtools copy and image verification remain necessary. Extraction
uses about 2.2 GiB of temporary disk space and reads the whole gzip, so it
also checks stream integrity rather than accepting a truncated head.

## Pinned identity

Verified against the GitHub release API on 2026-09-14:
<https://api.github.com/repos/ROCKNIX/distribution/releases/tags/20260901>.
The release's target commit is `1ebff24f36501fb6493beb2bf83bf2604536d9aa`.

| Artifact | SHA256 |
| --- | --- |
| `ROCKNIX-RK3326.aarch64-20260901-a.img.gz` | `e3272fc4266363332b830612db1abe4e20bb6a17c58d3c9f32815997556c768e` |
| Sectors 64 through 32767, inclusive | `0c88ade1572385515331cb6ea1c1ba6368c6ef867ef51ad59a57cafbfcff6c7a` |
| Original FAT `boot.scr` | `c61a5760f5073a81651620bd223541d267e5b0f0a9ed510609f6631275307531` |
| Patched `boot.scr`, mkimage 2025.10, `SOURCE_DATE_EPOCH=1` | `737d04649a77afb3160dc5fbcf39f80e9d45c74ee0fb87cfc29f8b904b7ea61c` |
| `20260901-b` archive; **not a substitute** | `f2b35e9feeef1a2ba2a40298614c1e9d67349ffd98e8498449e8815107ab82cc` |

The local `ROCKNIX-RK3326.img.gz` and saved loader/script were checked against
these hashes. The upstream `-a` download was also fetched and built through
the fixed-output derivation. U-Boot's legacy script header CRC, data CRC,
component length, and exact patch location are checked. Its original header
marks the script as gzip; like the proven procedure, mkimage retains that
metadata without compressing the script text. Do not reinterpret that header
as a gzip payload.

## Redistribution evidence and publication hold

**No cache publication is authorized by these helpers.** `allowSubstitutes =
false` only controls fetching this derivation's output. It does not prevent
someone from uploading it or its input closure.

Sources inspected at pinned revisions:

- [ROCKNIX LICENSE.md](https://github.com/ROCKNIX/distribution/blob/1ebff24f36501fb6493beb2bf83bf2604536d9aa/LICENSE.md)
  assigns original scripts to GPL version 2. Its branding/images are
  CC-BY-NC-SA 4.0; bundled works retain component licenses. Do not infer one
  redistribution license for the whole downloaded distribution.
- [The `-a` boot script source](https://github.com/ROCKNIX/distribution/blob/1ebff24f36501fb6493beb2bf83bf2604536d9aa/projects/ROCKNIX/devices/RK3326/packages/u-boot/config/a_boot.ini)
  matches the extracted script after the package's `ROCKNIX`/`STORAGE` label
  substitutions. The helper preserves its copyright header. `boot.txt`
  records the one-line ramdisk-address change as readable source.
- [The RK3326 U-Boot package](https://github.com/ROCKNIX/distribution/blob/1ebff24f36501fb6493beb2bf83bf2604536d9aa/projects/ROCKNIX/devices/RK3326/packages/u-boot/package.mk)
  identifies the `-a` artifact as the output of `u-boot-legacy`.
- [The legacy package](https://github.com/ROCKNIX/distribution/blob/1ebff24f36501fb6493beb2bf83bf2604536d9aa/projects/ROCKNIX/devices/RK3326/packages/u-boot-legacy/package.mk)
  pins `ROCKNIX/hardkernel-uboot` at
  `2492a3e467e332e2350d987234ce6123700b3392`. Its
  [Licenses/README](https://github.com/ROCKNIX/hardkernel-uboot/blob/2492a3e467e332e2350d987234ce6123700b3392/Licenses/README)
  permits redistribution under GPL v2, with per-file exceptions.
- [The ROCKNIX rkbin package](https://github.com/ROCKNIX/distribution/blob/1ebff24f36501fb6493beb2bf83bf2604536d9aa/projects/ROCKNIX/packages/tools/rkbin/package.mk)
  pins `74213af1e952c4683d2e35952507133b61394862`. Its
  [LICENSE](https://github.com/rockchip-linux/rkbin/blob/74213af1e952c4683d2e35952507133b61394862/LICENSE)
  explicitly grants permission to “use, copy, distribute the Software”. It
  forbids removing copyright/patent/trademark notices and restricts reverse
  engineering. The package's `nonfree` label does not mean redistribution is
  forbidden. The legacy recipe names DDR v2.11, miniloader v1.40, and BL31
  v1.34 from this source.

Unresolved before publishing: carry the applicable license texts/notices with
the extracted output, arrange GPL corresponding-source delivery (including
ROCKNIX patches and build scripts), and confirm component provenance for the
actual assembled loader. Recipe provenance is evidence, not a reproducible
rebuild of that binary. Do not upload the entire input closure: it includes
the full distribution with unrelated licenses and branding.

## Verification scope

The tests run on x86_64 without a device. Temporary FAT/ext4 files exercise
wrong and absent labels, the 48 MiB boundary, missing/corrupt modules and
indexes, extra entries, directory type changes, dangling links, link loops,
and changed closure/link provenance. The unit fixtures use real filesystem
trees with deterministic bytes, not module-ABI fixtures. A separate temporary
ext4 check also verified the installed Linux 6.18.17 module tree (7,303 module
files) and rejected a removed image module that still existed on the host.
That x86_64 tree is not the R36T Max kernel or a final-image acceptance test.

Real archived CI kernel, LZO initrd,
and board DTB files were also read with debugfs, copied to a new 128 MiB FAT,
and read back with mtools. Their archived APPEND matched that archived
system's `kernel-params`. That check used a **relocated boot-input fixture**;
it was not a build or acceptance test of the parent's final NixOS system.
The parent must build that system, run `verify-image` against the finished
artifact, and later perform physical acceptance. No card was present.
