# Native ARM64 Skate 3 recomp on the AYN Odin 2 Portal: level of effort

Researched 2026-10-01. Read-only. Nothing was built, patched, or deployed.

## Answer

The port is small. Upstream already did most of the ARM64 work. My estimate
is **2 to 4 working days to a playable build**, and **2 to 3 weeks in the worst
case**. The worst case happens only if the native Vulkan renderer misbehaves
on Mesa turnip and you must debug it.

| Phase | Effort | Confidence |
|---|---|---|
| Port 3 known upstream fixes into the SDK | 2 to 4 hours | High |
| First aarch64 build on `fuji`, fix compile and link errors | 0.5 to 1 day | Medium |
| Add `aarch64-linux` to `skate-3-flake` | 0.5 day | High |
| First run on the Odin, fix runtime faults | 1 to 5 days | Low. This is the big unknown |
| Tune settings for a 1080p handheld | 0.5 to 1 day | Medium |
| Korri plugin and delivery (not needed just to play) | Separate task | n/a |

## Result of the fuji experiment (2026-10-02, verified)

The build risk is closed. `simonwjackson/skate-3-flake` `13d4661` adds
`aarch64-linux` outputs and four ARM-only patches: rexglue-sdk `d82ec28`,
`85aa46b`, `96bee61`, and Buku313 `e99203e`. The x86_64 derivations are
unchanged.

| Check | Result |
|---|---|
| All patches apply to the pinned tree | Yes |
| Compile and link errors on `fuji` | 0. A manual `ninja -k 0` run took 41 minutes on 4 cores |
| Sandboxed `nix build .#packages.aarch64-linux.skate3-source` | Passes. About 39 minutes. Output `/nix/store/dsjk16fn…-skate3` |
| Generated C++, ARM64 compared with x86_64 | All 151 files byte-identical (SHA-256) |
| Libraries resolved with the binary's own glibc 2.44 loader | 73 resolved, none missing |
| Start without a display | Stops at `Failed to initialize GTK+`, as expected |

What remains is the on-device renderer risk on turnip. The output contains
generated EA code. It is only in `fuji`'s store and in
`fuji:~/build/s3arm/work`.

## Why it is small (verified)

**The SDK supports linux-arm64.** `mchughalex/rexglue-skate3` at `7eb0faf` has
a `linux-arm64` CMake preset and an ARM64 CI workflow
(`.github/workflows/build-linux-aarch64.yaml`, `ubuntu-24.04-arm` runner). It has
`REX_ARCH_ARM64` code paths in `src/core/exception_handler_posix.cpp` (aarch64
`ucontext` decoding), `src/system/mmio_handler.cpp`, `src/core/memory.cpp`
(NEON byte swaps), `include/rex/ppc/intrinsics.h` (NEON versions of VMX
shifts), and `include/rex/platform/fpscr.h`. Upstream `rexglue/rexglue-sdk`
publishes `linux-arm64` nightly builds. The fork is a squashed import of
upstream from 2026-05-18. Upstream ARM64 support landed on 2026-03-19
(`91781c0`), so the import contains it.

**The game already ships on ARM64.** `skate3recomp` v2.0.2 has a `Skate3Recomp-macOS.zip`
release, and the README calls macOS ARM experimental. `CMakeLists.txt` maps
`aarch64` on Linux to `linux-arm64`. No x86 intrinsics or architecture macros
appear in `skate3recomp/src`.

**The generated C++ does not depend on the host.** I checked the x86_64 build
on aka. `~/build/skate3recomp/generated/` has 151 files (289 MB). None of them
contain `__x86_64__`, `__aarch64__`, `_M_X64`, `_M_ARM64`, or `immintrin`.
Vector code goes through SIMDe (`simde_mm_load_si128` 189,021 times,
`simde_mm_shuffle_epi8` 64,503 times). SIMDe maps these calls to NEON on ARM64.
Guest `sync`, `lwsync`, and `eieio` become
`std::atomic_thread_fence(std::memory_order_seq_cst)`
(`src/codegen/builders/system.cpp`). So ARM's weaker memory ordering does not
break guest locks, provided the game itself uses barriers correctly.

**Other people run this exact code on Snapdragon.** Buku313's
[Skate3-Mobile](https://github.com/Buku313/Skate3-Mobile) forks `skate3recomp`
at `f6e0ae8`. That is the same commit that `skate-3-flake` pins. Its project page
reports about 90 FPS on an AYN Thor (commit `ba6def9`). Its release notes
identify the Thor GPU as an Adreno 740 (`docs/RELEASE_NOTES_v2.0.15.md`). The
Odin 2 Portal has the same Snapdragon 8 Gen 2 and Adreno 740. darchap's
[Skate3-Port](https://github.com/darchap/Skate3-Port) reports a 57.1 FPS average
on a Poco F3 (Snapdragon 870) at default settings. These numbers are from
Android with the Qualcomm driver, not from Linux with turnip. The profile used on
the Thor is not stated.

**The Odin is a simple target.** `nix/devices/odin2portal/kernel/config` sets
`CONFIG_ARM64_4K_PAGES=y`, so the 16 KB page work that Android needed does not
apply here. The device config evaluates `mesa` 25.3.2 and enables turnip.
`services/inputd/nix/korri-linux-host-core.nix` starts Sway with
`xwayland force`. The fork's Linux UI uses GTK and an XCB Vulkan surface, so it
needs Xwayland, and Korri provides it. The panel is 1920x1080 after rotation
(`web-session.nix`). That is 16:9, so the ultrawide and Vert+ code does not apply.

**Turnip has the Vulkan features the SDK checks.** Mesa 25.3.2
`src/freedreno/vulkan/tu_device.cc` enables `independentBlend`,
`geometryShader`, `tessellationShader`, `fragmentStoresAndAtomics`,
`vertexPipelineStoresAndAtomics`, `fillModeNonSolid`, `sampleRateShading`,
`depthClamp`, `textureCompressionBC`, `shaderCullDistance`, `nullDescriptor`,
and `customBorderColorWithoutFormat`. Mesa's `docs/features.txt` lists turnip as
complete for Vulkan 1.3 and complete for 1.4 on a7xx. It also lists
`VK_EXT_custom_border_color`, `VK_EXT_robustness2`,
`VK_EXT_non_seamless_cube_map`, and `VK_KHR_push_descriptor` for turnip. The
native renderer uploads BC1 to BC5 textures directly
(`src/graphics/vulkan/native_rhi_vulkan.cpp`), and turnip supports them.

## Known gaps (verified)

The fork has never been built for linux-arm64. Three upstream fixes that came
after the import are missing:

| Upstream commit | Problem it fixes | Effect on the Odin |
|---|---|---|
| `d82ec28` FFmpeg hidden visibility | FFmpeg NEON assembly cannot link into a shared library on linux-arm64 | `librexruntime.so` probably fails to link (inferred). macOS uses a different linker, which may be why the fork never hit this |
| `85aa46b` (#389) `memmove` in XEX delta patches | Overlapping `memcpy` corrupts the title-update patch on aarch64 | Title Update 3 applies wrong. The fork still has `memcpy` at `src/system/lzx.cpp:166` and `src/system/xex_module.cpp:316,427` |
| `96bee61` (#387) suspended-thread race | A thread created suspended can wait forever | Random startup hangs, more likely on ARM timing |

Buku313's SDK fork (`Buku313/rexglue-skate3-android`, branch `android-arm64`)
carries the same fixes as `bdc4901` and `53e4dda`. That is two independent
confirmations that these fixes matter on ARM64.

## Risks (inferred)

1. **Native renderer on turnip.** The Android forks used the Qualcomm driver by
   default and offered turnip only as an experiment. They also had to port
   "Vulkan layouts and resource lifetime fixes" (`19050db`, 2,624 lines) and
   "Guard Vulkan pipeline variant binding" (`e99203e`). Some of that may be
   needed on turnip too. This is the biggest unknown and the source of the
   2 to 3 week worst case.
2. **Startup hangs.** Skate3-Mobile QA 7 targets startup hangs on the AYN Thor.
   It moved boot movies to the emulated path and delayed pipeline compilation.
   Those hangs were with the Qualcomm driver. Turnip may behave differently.
3. **Sparse buffers.** The Android fork turns off sparse residency on Qualcomm
   (`eed3978`). Turnip exposes `sparseBinding` only when the kernel supports it.
   The SDK has a non-sparse fallback, and `vulkan_sparse_shared_memory = false`
   forces it. Low risk.
4. **Performance on Linux.** The 90 FPS Thor figure comes from Android. Linux
   cpufreq, GPU devfreq, and thermal limits on the ROCKNIX-based kernel can
   differ. Your aka settings (8x MSAA, 8192 shadow map, full-resolution SSAO,
   5x draw distance, `resolution_scale = 2`) will not fit a handheld. Start from
   `resolution_scale = 1` (1280x720 guest output) with MSAA, SSR, shafts, and
   HDR off.
5. **RAM.** The Odin 2 Portal comes with 8, 12, or 16 GB. I do not know which one
   you have. The guest backing object is about 4.5 GB of sparse mapping
   (from `eed3978`'s comment). 8 GB should work but leaves less room for Korri.
6. **Upstream fixes after the import.** The fork has its own squashed history,
   so Git cannot list what it lacks. I checked only upstream commits since
   2026-05-01 whose subjects mention ARM, aarch64, races, POSIX, or Linux.
   Others may matter.

## Build route

Do not build on the Odin. Two routes fit the Korri rules:

| Route | Cost | Notes |
|---|---|---|
| Native build on `fuji` (4 Neoverse-N1 cores, 23 GB RAM, 66 GB free on `/`) | Simplest. About 1 to 2 hours per clean build (inferred) | aka's ninja log totals about 7,500 job-seconds on a Ryzen 5 7600X, and that log includes some repeated steps. The ROCKNIX kernel is cross-built only because it needs about 30 GB of disk |
| Cross-compile on aka | 1 to 2 extra days of Nix work | `pkgsCross` dependencies (GTK3 and others) are not in cache.nixos.org, and codegen must run on the build host. Not worth it while `fuji` works |

The flake change is small. `flake.nix` hard-codes `system = "x86_64-linux"`, and both
package files set `platforms = [ "x86_64-linux" ]`. `package-source.nix` calls
`requireFile` for `default.xex` and `EAWebkit.xex`, so those files must be in
`fuji`'s store before the build. The output contains generated EA code, so it
must stay out of any public cache. Copy it to the Odin over a private route.

## Alternatives considered

| Option | Verdict |
|---|---|
| Port Skate3-Mobile instead of upstream | Not first. It adds 69 game commits and 26 SDK commits, mostly for Android (SDL Android, ashmem, touch, mods, crash upload). Its handheld profiles are worth adding later: 3D scene resolution from 512x288 to 1280x720, and crowd and traffic cuts |
| Run the x86_64 build under FEX or box64 | Not recommended. The recompiled code is mostly SSE through SIMDe, and every instruction would go through translation again. I did not measure it or check it in nixpkgs |

## First experiment

Build upstream `skate3recomp` `f6e0ae8` with `rexglue-skate3` `7eb0faf` and the
three fixes above on `fuji`, outside Nix, with the `linux-release` preset. Count the
compile and link errors. As a free correctness check, run codegen on `fuji` and
compare the generated files with aka's `generated/` tree. They must be
identical. If they differ, the aarch64 codegen path has a bug.

That takes about half a day. It removes the build risk and leaves only the
on-device renderer risk. The second experiment is a tethered run on the Odin
under Sway and Xwayland, with `vulkaninfo` first.

## State notes

- `skate-3-flake` HEAD is `e4f402f` (Vert+). The Korri plugin pins `e309e45`.
- The Skate 3 plugin commit `dc74019` in `korri-os/plugins` was reverted by
  `525fe5e` on 2026-10-01 22:54 -0600. The personal `simonwjackson/korri-plugins`
  repository has only its initial commit. An ARM64 plugin needs a plugin home
  first.
- aka's `~/build/skate3recomp` holds uncommitted Vert+ work. Leave it alone.

## Sources

- `mchughalex/skate3recomp` `f6e0ae8`: `README.md`, `CMakeLists.txt`, `CMakePresets.json`, `cmake/CodegenTargets.cmake`, `src/`.
- `mchughalex/rexglue-skate3` `7eb0faf`: files cited above.
- `rexglue/rexglue-sdk` `main` (`c94f5eb`, v0.10.0) and commits `85aa46b`, `96bee61`, `d82ec28`, `91781c0`. Releases API for `linux-arm64` nightlies.
- `Buku313/Skate3-Mobile` `e1b28c1` and `Buku313/rexglue-skate3-android` `edd4344`.
- `darchap/Skate3-Port`, `andrewnakas/skate3-android`, `AlanConstantino/skate3-pocket` READMEs.
- Mesa `mesa-25.3.2`: `docs/features.txt`, `src/freedreno/vulkan/tu_device.cc`.
- Korri: `nix/devices/odin2portal/`, `services/inputd/nix/korri-linux-host-core.nix`, `nix/device-cache/README.md`.
- aka read-only inspection: `~/build/skate3recomp/generated/`, `out/nix/.ninja_log`, `~/.local/share/skate3/settings.toml`.
- DROIX product page for Odin 2 Portal RAM variants (8/12/16 GB).
