# Validation CI

`Korri checks` and `Published plugin acceptance` only validate. They have
`contents: read`, no publisher secrets, no release upload, and no device effects.
The signed plugin pins, signed-cache trust, lifecycle VM and negative interface
controls in published acceptance are unchanged.

## Modes and source identity

| Trigger/mode | Selection |
| --- | --- |
| Pull request | Conservative merge-base diff; both paths of a rename and deleted paths count. The optional `ci-full` PR label forces full mode. No label is created by CI. |
| Push to `main` | Conservative affected selection from the event's `before` SHA; unavailable history selects all lanes within the affected profile. |
| Weekly schedule | Full, including published acceptance. |
| Manual `full` (default) | Every lane in that workflow. Dispatch both validation workflows on the same PR SHA for a full hosted comparison without publishing. |
| Manual `affected` | Diff if a base is available; otherwise all lanes within the affected profile. |
| Manual `probe` | Apply the real selector to `probe_paths`, a nonempty JSON array. This measures the selected paths on the tested source; it is not proof about an arbitrary commit's actual diff. |

Every checkout uses `source_sha` (a full 40-character commit SHA) or the event's
immutable `github.sha`. Each job verifies `HEAD` before running a check. On PR
events this is the tested merge commit, not a moving branch. A manual SHA must
contain these validation scripts. No source values are fetched from a publisher.

Manual runs can use the same source SHA with `full` and `probe`, for example
`probe_paths=["clients/portal/src/surface/settings-model.ts"]`. Other useful
probes are `docs/agents/domain.md`, `surfaces/pico/src/pico-settings-view.ts`,
`services/korrid/src/main.rs`, `contracts/generated/korrid.ts`,
`nix/devices/rpminiv2/kernel/default.nix`, and `flake.lock`.
Manual timing runs do not cancel each other. New automatic runs cancel older
validation runs for the same event/ref; these workflows never cancel publishers.

## Selection and coverage

`.github/ci/check-plan.py` owns selection and validates aggregation. Tests run
before CI applies omissions. Unknown paths, contracts, Nix composition, validation
workflow changes and any dependency lock select **all** lanes within the requested profile. The known cache
workflows, read helpers and their tests select `policy` plus `cache`. The trust
script and snapshot preparation script/tests also select their actual published
acceptance consumer. Cache-only inputs do not select Rust compiler jobs. Shared
cache trust identity/composition still selects full. A missing merge base or
failed diff selects all lanes without changing the requested profile. Invalid probe input or selection errors fail closed.
The selector starts from a two-commit checkout. If the merge base is missing, it
fetches at most 64 commits for each exact event-base/tested-head SHA, with a
30-second read timeout. It never fetches all history. A bad `before`, failed
fetch, or still-missing merge base selects all lanes within the requested profile. Selector logs report
selection elapsed time, including any bounded fetch; checkout and selector-test
time remain separate steps in the hosted job timings.

Known portal presentation, CSS, semantic input, overlay and surface code select
frontend/surface checks without Rust compilation. The separate Python `parity`
lane runs only when one of the five producer files read by
`clients/cli/tests/parity.py` changes, or the parity guard itself changes. Those
files are `settings-model.ts`, `plugin-repositories.ts`,
`launch-settings-model.ts`, Pico's `pico-settings-view.ts` and Zone's
`plan/options.ts`. Transport inputs under `clients/portal/src/korrid/` still
select real korrid RPC smoke and CLI/backend validation. Unknown portal, surface
or package inputs broaden to full; there is no blanket frontend-directory skip.

Ordinary affected/PR runs intentionally use Inputd's **fast profile**: Rust/core,
wasm and deployment-shell checks, without module/package/VM evaluation. Its Rust
crate consumes its core and `contracts/input`. Inputd changes select korrid
because korrid embeds the seat runtime. Ordinary korrid edits do not select the
fast Inputd profile; `services/korrid/src/host/systemd_unit.rs` does, because the
deployment-shell fixture reads its sandbox properties.

The **complete Inputd command does depend on `korridPackage`**, through
`services/inputd/nix/default.nix` and its bundle/package/Linux-module/seat-VM
consumers. The earlier claim of no general dependency was wrong for that command.
The 590-second backend probe omitted these checks and cannot prove full-coverage
selection. The user chose speed-first: the new PR module/VM requirement is deferred,
not treated as dependency-free. Manual `full`, the weekly schedule and the explicit
`ci-full` label run every Inputd check. Unknown/shared paths and missing history
broaden affected lanes but never opt an ordinary run into this heavy profile.

Only four named operation documents can omit product lanes: this file and the
three existing `docs/agents/` documents listed in the selector. There is no
`docs/**` exclusion. In particular, the RetroArch QuickJS probe, RetroArch route
and Android schema checkpoint under `docs/research/` are test inputs. The Nix
game-library ACL policy and generated treaty are also test inputs and select all
consumers. New documentation is unknown until reviewed, not silently skipped.

| Lane | App / retained coverage |
| --- | --- |
| `policy` (always) | `ci-plan-check`: selector/aggregate edge cases, workflow topology and syntax, shell lint, real Typeshare drift behavior. |
| `cache` | `korri-nix-cache-check` and `rpminiv2-delivery-check`: cache protocol/read tests, workflow snapshot preparation tests and `checks.<system>.rpminiv2-delivery` workflow safety/failure propagation. These validate locally and do not publish or prefetch production closures. |
| `fmt` | `korrid-fmt-check`: Rust formatting without compilation. |
| `treaty` | `korrid-treaty-check`: generate into a temporary file, compare without repairing drift, then check local and pinned plugin declarations. |
| `portal` | `portal-check`, `portal-bundle`: portal typecheck **before** Bun tests, then installed bundle. |
| `parity` | `cli-parity-check` / existing Python producer guard, without Nix setup or Rust compilation in the hosted job. Standalone `cli-check` also retains its original parity guard. |
| `cli` | Existing `cli-check`: parity, fmt, clippy, real-process tests. `cli-build-identity-eval-check` checks dependency reuse and final derivation identity through `checks.<system>.korri-cli-build-identity-eval`, without compiling two stamped packages. |
| `korrid` | `korrid-host-check`: pinned fixture hashes, script setup, all Rust tests, config review, dev binaries, socket-activated signer and real portal RPC smoke. No individual Rust-test selection. |
| `inputd` | Affected/probe: `inputd-check -- --fast` retains all Rust/core, wasm, portability and deployment-shell checks. Explicit full/scheduled/`ci-full`: default `inputd-check` also retains all eight module/package/VM gates. |
| `limitsd` | Existing `limitsd-check`: fmt, clippy, real filesystem/socket tests. |
| `pico`, `zone`, `boxbuster` | Their existing surface check apps, separate from portal tests. |
| `acceptance` (other workflow) | Existing `korri-published-plugins-check`: both signed architecture pins, stability, launch/interface negative controls, image/plugin checks, lifecycle VM. |

Portal, formatting and treaty jobs depend only on the small selector job, not
on Rust compilation or Nix store snapshots. Portal setup shares the portal's
React instance with every registered surface, in standalone and full checks.
Boxbuster's Bun preload resolves Fiber to its declared real ESM entry, as Vite
does. The renderer-entry regression test checks real export identity. It replaces
no renderer behavior and does not use a test double.
Korrid compilation/test/config/smoke stay together to avoid redundant cold Cargo
builds. The acceptance snapshot remains confined to its own job.

The retained korrid, portal and treaty lanes keep the original **30-minute** host
deadline; formatting is limited to 10 minutes. No test timeout is raised. The
additional CLI, surface, limits and cache lanes have 30-minute budgets. The new
Inputd lane retains a 45-minute budget for complete runs. Its new module/package/VM
coverage dominated the same-source full comparison: 1838 seconds versus 702 seconds
for the old checks, with a 1782-second Inputd job. Ordinary PR/affected runs now
exclude only that new heavy coverage, as the user requested. Complete runs still
include every assertion. The narrowed source needs new hosted qualification;
these measurements are not a speed claim for the new profile.

The stable `Host, portal, and surface` aggregate always runs. It requires the
selector to succeed and every selected lane to report success. Missing, skipped,
failed or cancelled selected lanes fail the aggregate. Unselected lanes must
report exactly `skipped`. Published acceptance has one always-reporting job named
`Published plugin acceptance result`. That job verifies source identity, runs
selector tests/selection, conditionally runs every heavy acceptance/cache step,
and always validates the actual `plan.outcome` and `published-plugins-check.outcome`
through the existing published aggregate contract. Selected missing, skipped,
failed or cancelled checks fail; only a planned skip passes. Earlier failed
steps still fail the native job, including snapshot preparation/save failures.
No result is replaced with a synthetic success or a step conclusion.

The two workflows retain independent required results. There is no cross-workflow
polling or extra acceptance selector/aggregate runner allocation. No protection
or ruleset settings are changed; use these stable result names when configuring
required checks.

## Build-host portability

Inputd's dev shell supplies real immutable `true` and shell executables from its
native Nix packages. Tests do not depend on a NixOS `/run/current-system` link.
Executable identity, store-item confinement and privilege assertions remain in
place. Its private DBus test uses the daemon package's native session policy and
explicitly binds only the fixture's temporary socket, never a host bus address.
For the full profile only, the hosted Inputd job uses the same real KVM permission,
udev settle and read/write prerequisite as published acceptance, before installing
Nix. Missing KVM fails that full job; no system-feature override substitutes for
actual access. Signed native Sunshine preparation remains immediately before
module evaluation. The fast profile needs neither KVM nor published plugin inputs.

Zone's check supplies the locked `NIX_PATH` to its existing Nix-shebang font
builder. It does not require a runner channel. Builder failures include stderr;
the deterministic TrueType/WOFF2 byte comparisons and timeouts are unchanged.
A focused native-builder replay, including Zone typecheck and lint, is:

```sh
NIX_PATH= nix run .#zone-check -- test/zone-caps-font.test.ts
```

Without Bun test arguments, the normal Zone lane still runs every Zone test.

## Local verification

Focused CI policy checks:

```sh
nix run .#ci-plan-check
nix run .#korrid-fmt-check
```

`nix run .#inputd-check` remains complete by default. The opt-in fast command is
`nix run .#inputd-check -- --fast`; it does not replace any Rust/wasm/shell assertion.
`ci-full-check` calls the complete default, not the fast profile.

The existing full korrid gate remains available as `nix run .#korrid-check`.
It now rejects stale generated treaties instead of silently rewriting them.
Deliberate regeneration remains `nix run .#korrid-check -- --types-only`.

The normal required CLI lane, including hosted full mode, uses the fast identity
evaluation check. Real-binary stamp proof remains an explicit packaging/release
qualification:

```sh
nix run .#cli-build-identity-check
```

This builds `checks.<system>.korri-cli-build-identity`, which compiles and tests
two stamped packages. It can cost two cold builds and does not run on every PR.
The coordinator can reuse its proof only when the packaging inputs match exactly.

Run all documented validation lanes and the real-binary packaging proof once on
the combined source. This also runs `korri-nix-cache-check` and the existing
`rpminiv2-delivery` check once each:

```sh
nix run .#ci-full-check
```

The local task runs lanes in order so heavy test processes do not compete on one
machine. Hosted full mode still runs independent jobs; this local command is not
a hosted latency sample. This is a build-host operation. Published lifecycle acceptance needs accessible
x86_64 KVM and the exact signed published closures. It does not deploy anything.

## Latency evidence

One observed full acceptance sample per source showed the cost of separate
selector/aggregate runners:

| Hosted run / source | End-to-end | Acceptance job | Runner allocations |
| --- | --- | --- | --- |
| `40540624581` / `3601c3610` | 1081 s | 1078 s | 1 |
| `40561086756` / `5faccef22` | 1138 s | 1111 s | 3 |

The observed 57-second difference comprises a 33-second acceptance-job difference
and 24 seconds outside that job. Different-source single samples do not prove
causality or a runtime budget. The one-job acceptance layout is a prototype that
removes allocation boundaries, not a claimed measured saving. Repeat comparable
full and affected/probe runs before landing; retain it only with hosted evidence.
The acceptance commands, signed trust, snapshot behavior, KVM prerequisite and
60/40/3-minute job/check/preparation deadlines remain unchanged.

Selection tests prove coverage relationships, not hosted latency. Before landing
a topology change, compare the same source and runner class in full and path
probe modes. Record first actionable error, aggregate elapsed time, setup,
transfer/import, runner minutes and peak slots. Device readiness belongs to the
separate delivery workflow. Include warm samples and a cold family/shared-input
sample; repeat suspected regressions. Historical timings are leads, not budgets.
Parallel lanes consume more simultaneous runners and can queue when the account
has fewer slots. Keep the independently measured improvements and defer any
unproven topology rather than claim savings from job counts alone.
