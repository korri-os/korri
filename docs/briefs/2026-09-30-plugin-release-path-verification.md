# External plugin release-path verification

Batch 4 is incomplete. Local release preparation and offline acceptance pass.
The final hosted x86 build and lifecycle pass. Its actual artifact passes
production-bound signature and payload checks. Hosted ARM validation fails when
its software VM cannot reach a guest shell. No ARM artifact was produced.
Both hosted runs skipped publication. No further retry, live publication or
handheld installation is authorized.

## Sources and completed work

Core baseline is `80e15526dc7e5d6a004d2fa5ed1218e75b57d942`.
Publisher baseline is `6b813d524e737dac1345c84b2e63d65b47ec699d`.
Both local mains contain batches 1–3. Remote Core main remains `1c9499051`.
After separate user approval, publisher remote main advanced from `37d805c` to
reviewed `add556d61e4c6fe3a4a4f0d4e9dd4fb59342d088`, then separately approved
`a62bc0d5c8607803ac73e613a47c3d79104ef1e5` with the controlled Nix pin and test
improvements below.

This evidence is revision-bound. After the tested commits, another session advanced
local Core main to `93a77b2090506563991a2defd92c38db9cc7461f` with an inputd fix,
and publisher main to `0cbdfdd2167015f43dfeba00cd0bbcef79cc0d6a` with RetroArch
seat-pad changes. Those behavior changes are not covered by these release-path
receipts. The hosted artifact checks deliberately evaluate immutable `a62bc0d…`.

Publisher test commit `5cc4911b2adb2141f0dcbe3c3b2d1782a20f2b7e` adds
caller-owned compiler-input coverage to the existing churn regression. It also
records output paths separately from drv-file identity. No dependency version,
approval record, runtime policy, image pin, or exporter changed.

## Dependency changes on both architectures

The regression uses the actual 21-output Mini V2 selection and actual generated
builder manifests. Counts below match on x86_64-linux and aarch64-linux.
Declared entries can repeat the same frontend or settings output across plugins.

| Changed input | Wrapper drv files | Wrapper output paths | Declared dependencies | Inspected check outputs |
| --- | ---: | ---: | --- | --- |
| Unrelated Core documentation. | 0 | 0 | None. | None. |
| Generated launch-contract bytes. | 0 | 0 | None. | Typecheck only. |
| Builder derivation input. | 21 | 21 | None. | Shared builder gate. |
| Libretro helper or settings producer. | 19 | 19 | Nineteen settings references. | Typecheck and settings. |
| RetroArch declaration. | 1 | 1 | None. | None. |
| OpenSSH compiler flags. | 20 | 1, SSH | Fifty-seven drv entries change; only OpenSSH's output changes. | None; two check drv files change. |

The compiler fixture uses the caller-owned package set and pinned nixpkgs
`withCFlags` adapter with `-fno-omit-frame-pointer`. It keeps the same OpenSSH
source, patches, build inputs, and compiler closure. This tests compiler
configuration, not a compiler-version replacement or a nixpkgs upgrade.
Unapproved Sunshine compiler-input changes still fail its base-derivation gate.

OpenSSH also enters the game build graph through SDL, GTK, Netpbm, and a
fixed-output SVN fetch. That fetch retains its content hash. Its changed drv file
does not change the game outputs. Therefore drv churn alone does not establish
compilation, signing, or publication requirements.

The changed x86 OpenSSH output compiled on build host zao in 51.88 seconds,
with 234 GCC compilation commands in its native build log. A cached rerun took
0.06 seconds. Substituters and remote builders were disabled. The compiler itself
was reused. ARM compiler-input identities were checked, but that changed ARM
native output was not compiled.

Both supported churn apps pass, at 52.80 seconds on x86 and 44.22 seconds on ARM.
Host Nix 2.34.1 and pinned app Nix 2.31.2+1 report identical identities.
Host-free checks find no Core host, korrid, or inputd producer in plugin builds.

## Current signing and preparation

The actual publisher CLI consumes five already-realized outputs per architecture:
SSH, RetroArch, mGBA, Sunshine, and Tailscale. Their manifests name `@korri`.
The build commands use zero jobs, no builders, no substitutes, and no fallback.
They return the exact architecture path lists without compiling or downloading
selected packages.

| Local stage | x86_64-linux | aarch64-linux |
| --- | ---: | ---: |
| Selected path-list production. | 3.22 s. | 2.67 s. |
| Full temporary-key export and signing. | 127.42 s. | 125.48 s. |
| Exported transitive paths. | 426. | 427. |
| Uncompressed NAR bytes. | 1,579,372,280. | 1,650,292,984. |
| Full preparation without upstream omission. | 9.73 s. | 11.09 s. |

Actual combine passes in 0.62 seconds. Its union has 852 store-path records and
837 distinct NAR assets. Native recursive content/signature verification passes.
Both new offline archives contain the complete original signed metadata closure,
not NARs or private keys. Direct consumer tests explicitly pass each produced
archive with its original file-cache metadata. Both pass in 1.27 seconds each,
including missing/conflicting proofs, corrupt transitive signatures, wrong
StorePath, and a same-label wrong full key. Earlier worker invocations used
prepared metadata and synthesized an archive; these direct receipts close that gap.

New artifacts use `batch4-test-only` and `example.invalid` URLs. They are not
production release candidates. Signing uses one immutable, private snapshot of
the host validity/signature database. Before/after checks confirm that all 852
main-store signature sets remain unchanged. No temporary signature entered the
host database. The temporary private key was removed.

The current packaged cache CLI was subsequently built. It downloaded signed
stock `gh-2.83.2` and built only its shell wrapper. Its actual packaged `build`
command then reproduced both five-output path lists with builds and substitutes
disabled. Packaged validate, temporary-key export of the actual SSH configuration
output, prepare, and signature verification also pass in isolated state.
Wrapper realization is distinct from plugin compilation.

The unchanged publisher suite passes all eight tests. Effective cache policy
passes both tests. Existing CLI export tests and private-key, overwrite,
corrupt-payload, signature-conflict, revision/tag, and upload-order controls pass.
Remote-write tests use local fixture processes, not real GitHub releases.

## Production proofs and offline image installation

Existing production batches `build-e5e27ed406f2` and `build-9f946f33bd18`
provide the exact image selections. Complete bound-key/content checks pass for
21 roots and 445 transitive x86 paths, and 21 roots and 446 ARM paths.
Tests start with unsigned private image-like registrations and import only the
local original proof archives. Archive presence or a matching signer label alone
is insufficient.

Current Core `nix run .#korri-published-plugins-check` passes in 362.88 seconds.
It downloads exact outputs and runs the production offline VM with current Core
and pinned historical SSH/mGBA packages. This is not a lifecycle rerun of the
changed publisher wrappers. The VM test script finishes successfully in
172.74 seconds. It blocks external egress
before proof import and recovery, rejects corrupt approval and missing proofs,
checks SSH login/wrong-key refusal, disable/re-enable, reboot recovery, and the
korrid game registry. Device Nix cannot compile or dispatch builds.
The mGBA test checks route/callback output, not gameplay with a valid ROM.

Actual evaluated image configurations keep native path-only plugin contexts,
not plugin derivation dependencies. Each requires offline proof import before
host recovery. Their selections are unchanged.

| Product image | ARM plugin outputs |
| --- | ---: |
| RG353M. | 19. |
| RG DS. | 19. |
| R36T Max. | 16. |
| Retroid Pocket Mini V2. | 21. |
| AYN Odin 2 Portal. | 20. |

This verifies image composition, not five physical image boots or complete image
artifact delivery. R36T Max distribution remains blocked by its loader-license
preflight. RG DS has no automatic portal session in its current release notes.

## Publication requirements and limits

An unrelated Core change needs no plugin release. The tested RetroArch declaration
requires only its changed output. The compiler fixture requires only changed SSH
and OpenSSH outputs. The builder fixture changes all selected wrapper outputs,
but no declared native outputs. A contract-byte change alone requires typecheck
acceptance, not new packages. A genuine host operation-name break fails acceptance
of the unchanged published callback.

Current publisher SSH and Sunshine wrapper paths differ from existing published
paths after ownership transfer. Their native identities match the published
manifests. RetroArch, mGBA, and Tailscale roots remain identical. Image pins and
proof hashes were not updated.

These observations identify affected package outputs. They do not prove minimal
live upload work. Export signs and serializes complete selected closures.
Preparation omits paths verified in the configured upstream cache, normally
cache.nixos.org. A new batch can still repeat unchanged custom NAR payloads.
The saved filtered Batch 1 artifact has 40 NAR assets. All 40 are byte-identical
in the current full preparation, totaling 25,067,484 compressed bytes. This is an
actual artifact comparison, not a fresh upstream probe or a live upload.
Append-only metadata retains the first URL for a shared path only when every
other field, including signatures, matches. A fresh batch still uploads its
prepared assets without comparing older batch payloads. Strictly minimal
cross-batch upload work is therefore not proven or implemented. No exporter
redesign was added.

Publishing builds retain publisher-plus-stock cache policy. Lifecycle can also
read Core cache. Mixing Core signatures into publishing exports can cause an
append-only conflict. Current warm-store test artifacts preserve those signatures
and must not be published. Publication is not atomic: metadata collision or
capacity refusal can follow a public NAR batch. Live release permissions, upload
races and live asset digest reporting remain unverified. A read-only metadata
snapshot at 2026-09-30T16:41:21Z found 242 cache assets against the 1000-asset limit,
leaving 758 slots then. This does not prove later capacity or write permission.
The final hosted x86 artifact now proves current full production-bound signatures
and prepared payload bytes. Its preparation omitted 313 stock paths after upstream
checks. Local temporary-key artifacts and missing current ARM artifacts do not
establish the corresponding ARM production signing or fresh upstream result.

Independent plugin closures can retain old dependency versions and use more disk.
Core updates do not apply plugin dependency security updates. The offline proof
service retains default closures in the system generation after uninstall.

## Measured hosted CI

User ask `149857a3-45df-49af-a0b7-b86848bfb02b` approved the publisher source-only
push and non-publishing CI. Run [36745677457](https://github.com/korri-os/plugins/actions/runs/36745677457)
checks `add556d61e4c6fe3a4a4f0d4e9dd4fb59342d088` on native x86 and ARM runners.
It selects only the changed SSH/Sunshine wrapper outputs and sets `publish=false`.
The first run failed overall. Measured stages are below; skipped stages are
not zero-time successful builds. No artifacts were produced.

| First run stage | Native x86 | Native ARM |
| --- | ---: | ---: |
| Scoped input invalidation | Passed, 1m36s | Passed, 1m27s |
| Broad validation | Failed, 11m59s | Failed, 11m52s |
| Selected-package build | Skipped | Skipped |
| Signing, preparation, upload | Skipped | Skipped |

The independent x86 generic lifecycle and input-presence job passed in 40m56s.
Publication was skipped. The native build jobs failed the same
`test-offline-retirement.cpp:417` assertion in the administrative offline-retirement
producer check. The exact immutable x86 package passed an actual local rebuild in
51s. A controlled unsafe-parent reproduction reached the same assertion with
`unsafe state ancestor ownership or permissions`; this proves an environment-sensitive
fixture failure, not the actual GHA directory metadata. No runtime security check
was weakened. A proposed sandbox `/tmp` fixture also failed and was not landed.
CI installed Nix 2.35.1; the verified local rebuild used 2.34.1. The version difference
is observed, not a proven cause.

User ask `4e486caa-0d8a-47c4-9f4f-7e68e12e3f37` approved one controlled retry
with Nix 2.34.1 pinned through the action's established `install_url` input.
Publisher commit `a62bc0d5c8607803ac73e613a47c3d79104ef1e5` pins all three Nix
steps and adds a positive retirement control below a safe 0700 ancestor, then
requires refusal after only that ancestor becomes 0770. The producer, original
build fixture, signing, owner guards, permissions and all release gates remain
unchanged. A parsed-workflow comparison verified that only the installer inputs
changed. Independent review found no remaining blocker in this bounded change.

The improved native x86 check passed in 61s. The installed full test, including
real xattrs outside Nix's syscall filter, passed in 0.37s. All five plugin output
paths are unchanged on both architectures. The standalone administrative helper's
input-addressed output paths change, but its actual x86 CLI binary is byte-identical
to the old binary. Both SHA256 values are
`4f3257f317a6f2532dc41c2bc69e49e65350bee9efee8c3d2a977f658b57809c`.
That executable comparison is not proof of unchanged whole-output NARs or ARM bytes.
Both-systems scoped invalidation passed again in 113s.

Retry [36754078980](https://github.com/korri-os/plugins/actions/runs/36754078980)
uses exact `a62bc0d5c8607803ac73e613a47c3d79104ef1e5`, the same two outputs,
and `publish=false`. The retry failed overall, after 70m28s of supervision.

| Pinned retry stage | Native x86 | Native ARM |
| --- | ---: | ---: |
| Scoped input invalidation | Passed, 1m10s | Passed, 1m24s |
| Broad validation | Passed, 8m54s | Failed, 68m12s |
| Selected-package build | Passed, 4s | Skipped |
| Signing and preparation | Passed, 1m36s | Skipped |
| Artifact upload | Passed, 2s | Skipped |

The independent x86 lifecycle and input-presence job passed in 51m56s.
Publication was skipped. ARM failed `vm-test-run-sunshine-input-seat-presence`
with exit 143 at its 3600s test timeout. The last driver messages repeatedly
report that the guest root shell has produced no data. The existing ARM test
explicitly selects QEMU TCG without KVM. This proves a guest-startup/driver
acceptance failure, not its root cause, an encoder failure, or ARM gameplay.
Do not remove the gate or claim that a larger timeout solves it.

The short x86 selected-output stage substituted OpenSSH and four SSH artifacts
from the signed publisher cache, then ran the manifest and SSH wrapper builders.
Sunshine's native output had already been substituted in broad validation.
Thus 4s is not native compilation time. Across the x86 build job, logs show 1302
distinct stock-cache paths and 18 publisher-cache paths substituted. Broad validation
launched 450 derivation builders, including checks and source/vendor preparation;
this count is not 450 compiler invocations. ARM broad validation launched 474
builders and substituted 1341 stock and six publisher paths before the VM failure.
Its later selected-output/signing stages did not run.

The only uploaded artifact is x86. The actual packaged `korri-cache combine`
accepts it alone in 0.16s; this is not a current two-architecture combine.
The direct original offline archive passes Core's unsigned-private-registration
proof tests in 1.42s for both current roots and all 327 transitive paths, using
`korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98=`.
Negative controls cover missing/conflicting proofs, corrupt transitive signatures,
wrong StorePath and same-label wrong key. The archive has only publisher and stock
signature names, not the CI test-only key. This is current production-bound x86
proof, not merely a matching label or the earlier temporary-key exercise.

All 14 downloaded custom NAR assets pass compressed FileHash/FileSize and
decompressed signed NarHash/NarSize checks in 0.75s. Their records match the
production-bound archive. They total 8,646,360 compressed bytes. The other 313
paths remain in the complete offline proof archive despite upstream omission
from public metadata/payload. NAR URLs name the unpublished verification tag;
these bytes and signatures do not prove live URL availability or installation.

Nix 2.35.1 compatibility remains unverified. Two source changes distinguish the
retry, so x86 success does not isolate the installer version as the cause.
No further retry, live publication or device deployment is approved.

Historical timings below are measured step durations, not predictions. Jobs
run concurrently and Core inputs differ.

The Core read-only acceptance workflow is not registered on its remote default
branch. No dispatch occurred. Pushing Core main to register it would trigger cache
publication, which remains unapproved. The executed local Core task is not a
passing hosted Core check.

| Stage | Baseline 36588085951 | Batch 1 36651820275 |
| --- | ---: | ---: |
| Cold-host lifecycle. | 38m04s. | 49m31s. |
| Broad x86 validation. | 20m24s. | 26m16s. |
| Selected x86 outputs. | 5m54s. | 7s. |
| x86 signing/preparation. | 2m47s. | 4m09s. |

Batch 1 ARM validation took 7m35s, selected outputs 6s, and signing/preparation
3m32s. Its logs show publisher substitutions and native derivation scheduling
inside broad validation. The short selected-output stage is not a cold compiler
time. No overall speedup is demonstrated. No live publication ran in that CI.

## Reproduction and remaining gaps

Run only on build machines:

```sh
# In Core.
nix run .#korri-published-plugins-check
# In the publisher.
nix run .#korri-plugin-churn-check
nix build --no-link .#korri-cache
```

Evidence is under `/tmp/korri-batch4-dependencies/`,
`/tmp/korri-batch4-release/`, and `/tmp/korri-batch4-core/`.
Entry points are `app-churn.json`, `bounded-build.json`,
`current-release-report.json`, `proofs-artifacts-report.json`,
`packaged-cache-report.json`, `review-followups.json`, `nar-reuse-report.json`,
`direct-produced-archive-*.log`, and Core `report.json` plus
`production-offline-vm.log`. Actual image contexts are in
`/tmp/korri-batch4-image-contexts.json`. Hosted run metadata, exact step timings
and logs are in `/tmp/korri-batch4-ci/publisher/` and
`/tmp/korri-batch4-ci-retry/publisher/`. The latter contains the sole downloaded
x86 artifact, `artifact-verification.json` and `log-observations.json`.
`/tmp/korri-batch4-ci/pin-verification/report.json` records workflow-policy
comparison, both-architecture output identities, the full installed native test
and actual x86 CLI byte equality. Core workflow non-registration is recorded in
`/tmp/korri-batch4-ci/core/unavailable.json`.

Generic ARM plugin lifecycle, ARM changed-native compilation, physical encoder
acceptance, ROM gameplay, physical image boots, handheld offline installation,
and live release publication remain unverified. Current hosted ARM input acceptance
also failed its software-VM startup timeout; current ARM signed artifacts and the
current two-architecture hosted combine are absent. Local temporary-key and
historical production ARM proofs are not substitutes for them. Obtain separate approval
before a Core main push, cache publication, image-pin update, or handheld operation.
