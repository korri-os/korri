# R36T Max RK915: build proof, not hardware proof

The image packages RK915 for its exact Linux 7.2.6 kernel, with the prior
proof's MMC quirks and PA5 host-wake binding. Public images omit the firmware.
Owner-provisioned blobs use Linux's native `/lib/firmware` search path. No live
Wi-Fi test has run on this build.

## Provenance and licensing

| Input | Grounding |
| --- | --- |
| Driver | `stolen/rk915`, commit `590fe1dd3fa9569117317b2e0dcbe02c42f8419e` (2025-07-08), fixed recursive SHA-256 in `source.nix` |
| Kernel | `../dts/kernel-trimmed.nix`, the repository's locked nixpkgs and existing board patches/configuration; no substitute kernel |
| Firmware | The two files in that commit's `firmware/` match the saved stock artifacts byte-for-byte |
| Hardware history | `legacy:docs/solutions/integration-issues/r36t-max-rocknix-rk915-wifi-25mhz-sdio.md` |
| Prior implementation | `/home/simonwjackson/code/sandbox/rocknix-r36tmax-proof`, `projects/ROCKNIX/packages/linux-{drivers,firmware}/rk915*` and RK3326 DTS/MMC patch |

The driver is Rockchip RK915 SDIO, **not Realtek**. Its compiled aliases are
`sdio:c*v0296d5347*` and `sdio:c*v0296d5348*`. The saved stock `rg42t.dts`
also identifies `wifi_chip_type = "rk915"` and `dwmmc@ff380000`.

Driver source carries GPLv2 notices; the repository's `LICENSE` is GPLv2.
**Firmware redistribution is blocked.** The pinned tree has two opaque binaries,
no firmware source, and no separate firmware redistribution grant. The proof's
`PKG_LICENSE="GPL"` is packaging metadata, not evidence that distributing those
binaries meets GPL source obligations. Do not infer permission from it.

`firmware.nix` therefore requires the already obtained files by exact hash.
It does not download them, vendor them, or install calibration data. The unfree
metadata is a conservative Nix policy, not an assertion that a proprietary
license was found. Builds require explicit unfree opt-in. This does **not**
resolve licensing or authorize redistribution. Keep firmware packages, images,
and the upstream source archive (which also contains the blobs) out of public
binary/source caches until permission/source obligations are resolved. Nix's
`allowSubstitutes = false` is not an upload ban.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `rk915_fw.bin` | 48056 | `f2d3a67176eaf06bce02a50ffc292c89c5f02fe33ba66daf18dec3389377e9e4` |
| `rk915_patch.bin` | 16716 | `d63f8d62e0ae0d7b21fd7b031a6e8360bcf90b722516c7e1278ed72c2e62c275` |

These names come from `src/firmware.c`'s `request_firmware` calls. The disabled
`ENABLE_FW_SPLIT` path also names `rk915_patch_cal.bin`; do not enable it or
invent that file. `rk915_cal.bin` is optional in the existing driver; no device's
calibration data is copied here.

## Linux 7.2.6 port

`linux-7.2.patch` follows the 7.2.6 kernel's `pm_wakeup.h`, `syscore_ops.h`,
`timer.h`, `string.h`, `moduleparam.h`, and mac80211 callback declarations.
Wakeup sources now use the public allocator. Allocation failure stops HAL
initialization before threads start. Later failure paths release the source;
normal teardown releases it after the work and TX/RX threads stop. Syscore
registration owns a `struct syscore` with a separate constant operations table.

The radio-index callback arguments do not add multi-radio support. RK915 does
not advertise wiphy radios and keeps its existing `hw->conf` behavior. Timer
renames retain asynchronous versus synchronous cancellation. RF parameter
copies retain their fixed-width bytes; only the driver name is a C string.
The namespace import now uses the required string literal. The Makefile uses
`ccflags-y`, because 7.2 no longer reads external-module `EXTRA_CFLAGS`.

Verified on the native AArch64 builder using the image configuration's actual
`boot.extraModulePackages`, not a substitute kernel:

- Output: `/nix/store/gqv13jvjj9b7chsys5c48ixvjgd9f6vd-rk915-7.2.6-unstable-2025-07-08`.
- `check-module.sh` passed after stripping, including all 162 imported symbol
  CRCs against the exact 7.2.6 kernel.
- `check-dtb.py` passed: 25 MHz SDIO, RK915 quirk, PA5 level-high host wake,
  and disabled eMMC.

Compiler warnings remain in the vendor source. No hardware, failure-injection,
suspend/resume, or unload test ran. This is module-build evidence only.

## Host-only build and checks

Run from the repository root on x86_64 Linux. The commands disable remote
builders. They can download prebuilt dependencies; they never contact a device.
The initial dry run bounds work before compilation: if the exact kernel is not
cached, its build can be costly. Do not replace it with a similar kernel.

```sh
nix build --impure --file nix/devices/r36tmax/wifi/packages.nix \
  driver --no-link --dry-run --option builders ''
nix build --impure --file nix/devices/r36tmax/wifi/packages.nix \
  driver --no-link --print-out-paths --option builders '' -L
```

For local testing only, after reading the licensing restriction above:

```sh
nix-store --add-fixed sha256 ~/.local/share/korri/rk3326-stock-shell/artifacts/rk915_fw.bin
nix-store --add-fixed sha256 ~/.local/share/korri/rk3326-stock-shell/artifacts/rk915_patch.bin
NIXPKGS_ALLOW_UNFREE=1 nix build --impure \
  --file nix/devices/r36tmax/wifi/packages.nix \
  firmware checks --no-link --print-out-paths --option builders '' -L
```

The driver installs only `lib/modules/<kernel-release>/extra/rk915.ko`.
Firmware installs only the two files under `lib/firmware/`. Native tools inspect
the module after stripping, even in a cross build. Checks cover AArch64 ELF,
release/vermagic, GPL metadata, both SDIO aliases, the three module parameters,
firmware names, and every imported symbol CRC against this kernel's
`Module.symvers`. The earlier 6.12.63 build had 159 imports; its separate
`nix build --rebuild` produced the same module output with no reproducibility
error. The 7.2.6 build above has 162 imports; it has not had a rebuild comparison.

The `checks` output also rejects absent modules, wrong release paths, a changed
`module_layout` CRC, and missing/corrupt firmware. It compares packaged firmware
with the pinned upstream files. Finally, it dry-runs the upstream MMC patch
against the exact Linux source with zero fuzz. On 6.12.63 all hunks apply; one
has a four-line offset. This does not apply the patch to the built kernel.

Missing local firmware and the default unfree restriction were also tested:
both fail closed. The executable `check-module.sh` and `check-firmware.sh` use
pinned Nix shebangs for standalone artifact inspection. No test loads a module.
The vendor build emits warnings; compilation is not a driver safety audit.

## Integrated radio contract

Use the image's **own** `config.boot.kernelPackages.callPackage ./wifi/driver.nix
{ }` for `boot.extraModulePackages`. Keep the kernel and module together; matching
`uname -r` alone is insufficient with `CONFIG_MODVERSIONS=y`. Add the firmware
package to `hardware.firmware` only for an authorized local image. There is no
firmware/image redistribution clearance yet.

The prior proof's module settings come from its vendor test-script usage:

```conf
options rk915 down_fw_in_probe=1 default_phy_threshold=180 lpw_no_sleep=1
```

Use `boot.extraModprobeConfig` for these settings. SDIO aliases support autoload;
if the image explicitly loads `rk915`, make sure the options and firmware are
available first. This slice does not change boot ordering or networking policy.
No SSID, password, connection profile, or interface naming rule is added.

The image carries these prior-proof constraints. Supply differences remain
unresolved; they are not changed merely to match labels:

| Constraint | Current board source | Integration gate |
| --- | --- | --- |
| SDIO clock | `max-frequency = <25000000>` | Preserve 25 MHz; prior 50 MHz reset failed with `-110` |
| Reset | GPIO0 PA2 active-low, 100 ms post-power-on delay | Preserve polarity; active-high broke prior enumeration |
| Host wake | PA5 pinctrl group and function-1 `wifi@1` child | Compiled DTB check verifies GPIO0 PA5 and level-high interrupt. |
| MMC quirks | `supports-rk912` and `mmc-support.patch` | Restricted to the marked SDIO controller. |
| Supplies | `vqmmc-supply = <&vcc1v8_soc>`, no `vmmc-supply` | Prior proof used `vcc2v8_dvp` and `vcc3v0_dvp`; these trees differ. Verify actual rails, do not copy regulator labels blindly |

The host-wake binding is grounded in the proof's
`rk3326-gameconsole-r36tmax.dts:62-69` and the driver's
`docs/mainline-linux-dts-example.dtsi`. Inside the current `&sdio`, use:

```dts
supports-rk912;
wifi@1 {
    reg = <1>;
    interrupt-names = "host-wake";
    interrupt-parent = <&gpio0>;
    interrupts = <RK_PA5 IRQ_TYPE_LEVEL_HIGH>;
    pinctrl-names = "default";
    pinctrl-0 = <&wifi_host_wake>;
};
```

`wifi_host_wake` is the existing board label (the proof calls it
`wifi_host_wake_l`). The driver reads `host-wake` from the SDIO function's OF node
in `src/sdio.c`; merely declaring the PA5 pinctrl group does not bind it.

The pinned `docs/mainline-linux-hacks-for-rk915.patch` is present in the proof
as `009-rk915-mainline-mmc-hacks.patch`. Our `mmc-support.patch` adds one safety
bound before `buf[12]` and `buf[13]` are read. A shortened RK915 CIS tuple must
still contain 14 bytes. `check-cis.py` executes the real patched parser with
ASan/UBSan against short, RK915 and normal SDIO buffers. The patch adds the
`supports-rk912` consumer and changes SDIO CIS parsing, resume power handling,
CMD52 ordering, and DWMMC low-power clock behavior. It is a runtime dependency
of reproducing the prior proof, not a compile dependency of this module.
These non-upstream kernel changes are now wired into the image kernel.
Compilation and compiled DTB checks pass; SDIO enumeration remains untested.
No speculative register or regulator changes are included here.

## Remaining gates

- Resolve firmware redistribution/source obligations before public caching or
  shipping an image.
- Provision the owner firmware before the first radio boot. Repeat the module
  ABI and compiled DTB checks for every kernel change.
- On an approved device run, verify SDIO enumeration, firmware download/reset,
  scan, association, DHCP, and SSH. Use existing credential policy; no secrets
  belong in these packages.
- Verify repeated cold and warm boots, sustained traffic, reconnect behavior,
  and suspend/resume. Disabling radio sleep can increase power use.
- Upstream's pinned README warns of post-association disconnects and module
  unload stalls. Prior tuned hardware success does not remove those risks.
  Do not treat module unload as a safe recovery operation.

No device validation or deployment was performed. Parent integration owns the
image and any later authorized card/deployment operation.
