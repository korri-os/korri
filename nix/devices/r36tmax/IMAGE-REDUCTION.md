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

## Image measurements

The controlled comparison uses baseline `f3aa66d9` and candidate `1c853f10`,
with the same diagnostic SSH policy and no private provisioning. Both complete
images passed the native image verifier. The existing distribution command
recompressed both with `zstd -19 -T2`.

| Metric | Before, bytes | After, bytes | Reduction |
|---|---:|---:|---:|
| Raw SD image | 5,335,453,696 | 4,584,374,272 | 751,079,424 / 14.1% |
| Builder-compressed image | 1,034,857,085 | 987,291,805 | 47,565,280 / 4.6% |
| Equally compressed download | 810,989,562 | 773,457,839 | 37,531,723 / 4.6% |

Raw images include ext4 metadata and allocation slack, not only file payloads.
Removing the source tree also removes thousands of directory entries and files.
Do not report the raw-image difference as the NAR-closure saving.

Each staged download decompressed to exactly its original image SHA-256. The
candidate raw image hash is
`a6d17f466cede9d4431d1139dee5515bc510ab716494e39542b28809b4ab2bc9`.
Its distributed compressed hash is
`1e1bbdd8afdef4ae33efa66e0ec8a4981f092a5116db7fbf63f834191c34b25c`.
Compression and the existing revision/checksum verifier passed on the staged
candidate. The code commit is `1c853f1084360abbae87467b495964b8af07330a`.

Local evidence and artifacts:

- `/tmp/r36tmax-reduction/` contains baseline/candidate evaluation and recursive
  closure records, the source-guard rejection log, image hashes and comparisons.
- `/tmp/r36tmax-reduced-dist/` contains the candidate image, checksum and code
  revision. These local artifacts are not a public release or boot acceptance.

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
Only after separate approval and verification of a spare removable SD device,
the existing writer command is:

```sh
sudo nix/devices/r36tmax/write-image \
  /tmp/r36tmax-reduced-dist/nixos-r36t-max.img.zst \
  "${VERIFIED_SD_DEVICE:?Set this only after live reader identification}" --i-know
```

This command erases that SD card. It does not provision keys or Wi-Fi firmware.
Normal generation installation remains blocked for the preserved FAT loader.

Before accepting a new card, verify boot, USB recovery, Wi-Fi, panel output,
buttons and audio against the existing baseline. Do not unmask Chromium or run
codec tests as part of size verification; those have separate hardware holds.
