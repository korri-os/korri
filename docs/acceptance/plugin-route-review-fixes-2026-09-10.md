# Runtime route review fixes — 2026-09-10

Base: `6a4dc086`.

## Scope

Two fixes only:

- Ordinary `app.session.prepare` resumes the same running game even when its
  stored runtime is stale or its available runtimes are ambiguous. The host
  passes the command resolution result into the session mutex path. Only a new
  session inspects the resolution error. Explicit runtime launches still use
  the fresh-session path and reject route switches during an active session.
- The runtime-choice setter returns device/games revisions from the bytes it
  committed and read under its existing write lock. The host no longer rereads
  revisions after releasing that lock.

No schema, permission, plugin, script, reference, or generated-contract changes.
No VM, device, deployment, push, merge, or delegation was used.

## Test-first reproduction

Each test below ran before the production fixes. Only regression assertions and
a `cfg(test)` response-delay callback had been added at that point.

Command prefix:

```sh
nix develop .#korrid --command bash -c \
  'cd services/korrid; cargo test --lib <test-name> -- --exact'
```

| Exact test name | Before fix | After fix |
| --- | --- | --- |
| `portal_access::tests::installed_routes_persist_choices_and_launch_explicit_runtime_through_rpc` | Failed: ordinary prepare returned `LocalRouteUnavailable` after an explicit launch with a stale preference. | Passed: ordinary prepare returns the same session; repeated explicit launch still conflicts. |
| `portal_access::tests::ordinary_prepare_resumes_explicit_runtime_despite_ambiguous_routes` | Failed: ordinary prepare returned `LocalRouteUnavailable` with two installed runtimes after an explicit launch. | Passed: ordinary prepare returns the same session; selecting the other runtime still conflicts. |
| `host::tests::concurrent_runtime_choice_responses_keep_their_own_committed_revisions` | Failed: caller A returned caller B's device revision instead of A's committed revision. | Passed for both System and Game scopes; A's returned revision cannot overwrite B. |

The concurrency test calls the real `HostRuntime::set_game_runtime` and writes
real files in a temporary directory. It delays caller A after the setter
releases its lock. While A's response is held, another caller reads A's bytes,
commits the next choice, and changes the other document. Releasing A must return
A's original revision pair. A subsequent write using A's response must fail with
`SettingsConflict`. Channels control this ordering; no timing race or copied
setter implementation supplies the result.

Before-fix log: `/tmp/korri-review-before.log`.

## Validation

All commands ran in the pinned `korrid` Nix development shell, from
`services/korrid`.

| Command | Verified result |
| --- | --- |
| `cargo test --lib portal_access::tests` | 7 passed |
| `cargo test --lib host::` | 127 passed |
| `cargo test --lib config::settings::tests` | 6 passed |
| `cargo test --test runtime_choices --test minimum_config_regression --test plugin_route` | 1 + 15 + 12 passed |
| `cargo fmt --check` | Passed |
| `cargo clippy --all-targets` | Passed with existing warnings |
| `git diff --check` | Passed |

Total: 168 focused tests passed. This includes in-process authenticated RPC
requests, installed-package route resolution, real configuration writes, and
session-control regressions. It does not prove a native launch on a device.

Strict `cargo clippy --all-targets -- -D warnings` fails with 19 library findings
and 29 library-test findings. The same command on an unmodified archive of
`6a4dc086` fails with the same diagnostic messages and locations. No new clippy
finding was introduced. Existing findings include `type_complexity` at
`host/session_state.rs:146` and unrelated discovery, launcher, and RPC code.
Those findings are outside this slice and remain unchanged.

Validation logs: `/tmp/korri-review-after.log`,
`/tmp/korri-review-checks.log`, and `/tmp/korri-review-baseline-clippy.log`.

## Commit method

No dedicated commit tool is available in this session. Use the available Bash
tool as the fallback: stage only the five changed Rust files and this acceptance
record, inspect the staged paths, then run `git commit`. Do not stage the brief
or `/home/simonwjackson/code/sandbox/korri/progress.md`.
