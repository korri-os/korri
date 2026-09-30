# External plugin release-path verification

Local release preparation and offline acceptance pass on the current sources.
Current-revision hosted CI remains pending. Live publication and handheld
installation are not authorized or verified.

## Sources and completed work

Core baseline is `80e15526dc7e5d6a004d2fa5ed1218e75b57d942`.
Publisher baseline is `6b813d524e737dac1345c84b2e63d65b47ec699d`.
Both local mains contain batches 1–3. Remote Core main remains `1c9499051`;
remote publisher main remains `37d805c` at the start of this verification.

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
races, digest reporting, and current capacity remain unverified. Current production
private-key signing and fresh upstream availability were not tested locally.

Independent plugin closures can retain old dependency versions and use more disk.
Core updates do not apply plugin dependency security updates. The offline proof
service retains default closures in the system generation after uninstall.

## Measured hosted CI

Current-revision hosted CI is pending approval. Historical timings below are
measured step durations, not predictions. Jobs overlap and Core inputs differ.

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
`/tmp/korri-batch4-image-contexts.json`.

Generic ARM plugin lifecycle, ARM changed-native compilation, physical encoder
acceptance, ROM gameplay, physical image boots, handheld offline installation,
and live release publication remain unverified. Prior ARM Sunshine software-VM
input acceptance is not a replacement for those checks. Obtain separate approval
before a Core main push, cache publication, image-pin update, or handheld operation.
