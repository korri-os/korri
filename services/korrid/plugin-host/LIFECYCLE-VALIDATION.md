# Current/previous lifecycle verification — 2026-09-10

Implemented against `bde07bff` and the approved authoring brief's gaps 3 and 7.
The supplied worktree `context.md` and `plan.md` did not exist. The explicit
assignment and versioned brief defined the scope.

## Verified without a VM

| Check | Result |
|---|---|
| New receipt/filesystem tests, first run before implementation | Failed because the selection module did not exist |
| `nix develop .#plugin-host --command cargo test --manifest-path services/korrid/plugin-host/Cargo.toml` | 119 passed, 6 explicitly ignored |
| `nix develop .#plugin-host --command cargo clippy --manifest-path services/korrid/plugin-host/Cargo.toml --all-targets -- -D warnings` | Passed |
| `nix build --no-link .#checks.x86_64-linux.korri-plugin-host` | Passed, including sandboxed release tests |
| `rustfmt --check`, with `skip_children=true`, on changed Rust files | Passed; no formatting changes to shared korrid modules |
| `nixfmt --check` on changed Nix files; `git diff --check` | Passed |
| VM derivation evaluation and Python `ast.parse` of `config.testScript` | Passed; test script was not executed |

The package check first caught the missing JSON fixture in Crane's source
filter. The filter now includes that exact receipt fixture. The repeated
package check passed.

Eight receipt/filesystem tests cover bounded history, exact provenance swaps,
current desired state, failure before receipt commit, each root repair boundary,
interrupted swaps, temporary roots, completed removal, and strict receipt reads.
Dependency tests include rollback with enabled and disabled exact dependents,
and launcher/kind removal or rename without manifest `requires`.

## Final integrated gate remains unrun

No VM was started. Added VM assertions cover dependency-safe rollback, offline
swaps, failed explicit restore, failed/interrupted update retention, revocation,
and removal of previous roots. Existing admission-only game fixtures now name
real prebuilt `true` files and avoid a duplicate `launch` export. These fixtures
do not prove emulator launch or gameplay.

Deployment must explicitly cut over receipts to the required `previous` slot.
There is no compatibility read or runtime migration. Boot and operator recovery
use `restore-all`; `restore ID` swaps selections. Exact Nix requirements remain
mandatory. Previous closure presence does not select or approve a dependency.

## Commit scope

The session exposed no green-slice commit tool. Use the documented fallback:
standard `git add --` with explicit paths, then `git commit`. The path list is:

- `services/korrid/plugin-host/README.md`
- `services/korrid/plugin-host/LIFECYCLE-VALIDATION.md`
- `services/korrid/plugin-host/nixos-module.nix`
- `services/korrid/plugin-host/package.nix`
- `services/korrid/plugin-host/src/host.rs`
- `services/korrid/plugin-host/src/lib.rs`
- `services/korrid/plugin-host/src/main.rs`
- `services/korrid/plugin-host/src/selection.rs`
- `services/korrid/plugin-host/tests/dependencies.rs`
- `services/korrid/plugin-host/tests/host_boundary.rs`
- `services/korrid/plugin-host/tests/selection.rs`
- `services/korrid/plugin-host/tests/fixtures/selection.json`
- `services/korrid/plugin-host/vm-test.nix`

Exclude the copied brief and all files outside `services/korrid/plugin-host/`.
No merge, push, or deployment is part of this slice.
