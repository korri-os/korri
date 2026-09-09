# Progress

## Status
Rust worker complete — all six regression fixes verified. Parent review and shipping remain.

## Tasks
- Worker owns Rust only. Parent owns shell/Nix/docs and shipping.
- No staging, commits, delegation, or device effects.

## Verified RED
- `/tmp/minimum-core-red-1-4.log`: regression integration target 9 passed / 3 failed (implicit copy selection, Linux runtime ambiguity, authored physical alias). Host unit target 0 passed / 1 failed (retained unlocated game aborts startup).
- `/tmp/minimum-core-red-5-6.log`: document recovery target 4 passed / 2 failed (settings overwrites pending device publication; concurrent editor change clears journal).
- `/tmp/minimum-core-linux-edge-red.log`: host unit 0 passed / 1 failed. The Linux materializer also rejected explicit storage IDs after resolver selection. Updated this necessary caller to use the same authorized filesystem edge as Android.

## Implemented boundaries
- Resolver and catalog callers now supply the real readable root; missing implicit copies no longer win selection. Preserve LocalRomMissing and filter platform-incapable candidates before ambiguity.
- Linux host keeps valid games and returns route failures; schema/authorization and collisions remain fail-closed. Linux launch materialization resolves explicit and implicit storage through the existing storage edge.
- Reconciliation indexes storage/path and authored canonical files once, retains catalog facts and all copies.
- Settings takes explicit private root and shared lock, rejects pending publication before mutation.
- Recovery rereads all candidate bytes before clearing journal/ownership. Test-only publication hook exercises real commit boundaries and restart IDs.

## Verified GREEN
All commands used `nix develop .#korrid --command bash` and ran from `services/korrid`.

| Command / scope | Result |
| --- | --- |
| `cargo test` | 518 passed, 0 failed, 0 ignored across 23 targets (including zero-test binaries/doc-tests) |
| Library unit tests | 363 passed |
| Main binary unit tests | 19 passed |
| Integration tests | 136 passed |
| `cargo test --lib discovery::reconcile::documents::tests` | 6 passed |
| `cargo test --lib host::tests::linux_host_materializes_wario_from_the_shared_plugins` | 1 passed; latest collision assertions also passed in final full run |
| `cargo test --test retroarch_plugin_route` | 13 passed |
| `cargo test --test android_app_route` | 1 passed |
| `cargo test --lib launcher::tests` | 11 passed |
| `minimum_config_regression` in final full run | 14 passed, including real 10,000-file add/rescan; target took 19.44 seconds |
| `cargo fmt` then `cargo fmt --check` | Passed for the full crate |
| `cargo check --all-targets` | Passed |
| `cargo clippy --all-targets` | Passed with warnings; not warning-free |
| `git diff --check` | Passed |

Final log: `/tmp/minimum-core-final.log`. Targeted log: `/tmp/minimum-core-targeted.log`.

Additional RED preservation checks: `/tmp/minimum-core-collision-red.log` (missing media concealed a provider collision) and `/tmp/minimum-core-prepare-collision-red.log` (prepare returned HostGameNotFound instead of collision failure). Fixed collision precedence before byte observation and shared static/dynamic collision validation for catalog and prepare.

First full run exposed 9 older tests that listed/resolved nonexistent ROM fixtures. Added real ROM files only in tests that require availability. Missing-ROM tests remain intact. No fixture schema keys changed.

## Costs and limits
- The 10,000-file regression adds about 19 seconds to this run. It verifies behavior at scale, not a fixed timing threshold.
- Settings deliberately returns Conflict while discovery publication is pending; recovery must complete before a save can proceed.
- No device acceptance, deployment, staging, commit, or push performed. No users/fold/cards schema or host.toml migration added.

## Notes
- Supplied `context.md` and `plan.md` do not exist. Read the approved task, current Rust changes, and `docs/briefs/2026-09-06-config-cascade-discussion.md`.
