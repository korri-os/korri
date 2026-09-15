# R36T Max image reduction

This pass ports the RG353M's on-demand Nixpkgs registry. It uses the existing
lossless distribution command for the final download. It does not remove
running services or change the kernel.

## Measured baseline

The diagnostic system at `f3aa66d9` has a 3,518,795,000-byte NAR closure.
Its Nixpkgs source accounts for 194,197,184 bytes. The largest three packages
are Chromium, LLVM and Mesa. Those packages remain unchanged in this pass.

| System closure | Before | After |
|---|---:|---:|
| NAR bytes | 3,518,795,000 | 3,324,597,928 |

The closure shrinks by 194,197,072 bytes, or 5.5%. A recursive comparison found
only the source removal and changes to the native registry, `/etc` output and
system output. All other store paths are identical, including hardware drivers,
Chromium, Mesa, LLVM, Sunshine, korrid and recovery tools.

The handoff incorrectly claimed that this device included generic Linux
firmware. Evaluation of the actual configuration found only `wireless-regdb`.
`hardware.enableAllHardware` and `hardware.enableRedistributableFirmware` are
already false. There is no generic firmware package to trim. Adding the RG353M
firmware allowlist here would add files, not remove them.

## Changes and costs

| Change | Grounding | Cost and limits |
|---|---|---|
| Fetch Nixpkgs on demand. | RG353M commit `2fa7cc93` and `nixpkgs-registry.nix`; the native fetcher record comes directly from `flake.lock`. | An uncached `nixpkgs` alias lookup needs network access. Revision and NAR hash stay pinned. Nix and the download-only policy do not change. |
| Stage the final download with existing `zstd -19 -T2` compression. | `nix/formats/image-dist.py` and RG353M's controlled comparisons in `nix/formats/IMAGE-BUILDS.md`. | Compression takes more build-host time. It changes download size, not installed bytes or the kernel's LZO initrd. This is an existing distribution step, not a new size saving attributable to the registry change. |

The registry change applies to all four image variants. All builders retain
their existing compression settings. An experiment also applied level 19 in
the image builder, but that would duplicate the existing distribution step.
The final implementation keeps compression in `device-image-dist`. The separate
mainline-loader experiment and its boot-test scope remain unchanged.

The registry module replaces the complete native record, not individual fields.
The system derivation rejects the Nixpkgs source anywhere in its closure through
`disallowedRequisites`. Existing rollback generations or a later explicit lookup
can still retain a source copy on a device. This change runs no garbage collection.

## Verification

Run on a build host, never on the handheld:

```sh
nix build .#checks.x86_64-linux.r36tmax-registry \
  .#checks.x86_64-linux.r36tmax \
  .#checks.x86_64-linux.korri-base \
  .#checks.x86_64-linux.korri-sd-card --no-link
nix run .#device-image-dist -- packages.aarch64-linux.r36tmax-diagnostic-sd-image /tmp/r36tmax-reduced-dist
```

The registry check rejected the path-based baseline before the module was added.
It checks all four configurations, the generated registry JSON, and the real Nix
registry reader. A separate host test resolved the native alias in an empty
store with builds disabled and obtained the exact locked revision and NAR hash.
A negative system build restored the old source-path registry and failed at the
native closure guard. The image build runs the existing loader, FAT, kernel,
DTB and root-file checks.

## Next candidate

The closure also contains ordinary and headless FFmpeg. A read-only audit found
that ALSA plugins retain ordinary FFmpeg. Reusing headless there is a possible
28.9 MB dependency cut, not a measured saving. It needs ALSA/codec parity and
separate audio acceptance before changing the current speaker stack. Chromium,
LLVM, Mesa, speech/MIDI data and recovery packages are not proven unused.

## Hardware gate

A completed image build is not boot acceptance. No target writes, restarts,
service changes or private provisioning are part of this pass. Keep the working
SD card and internal eMMC unchanged. Follow `README.md` for owner provisioning,
live reader identification, complete-image writing and readback verification.
Normal generation installation remains blocked for the preserved FAT loader.

Before accepting a new card, verify boot, USB recovery, Wi-Fi, panel output,
buttons and audio against the existing baseline. Do not unmask Chromium or run
codec tests as part of size verification; those have separate hardware holds.
