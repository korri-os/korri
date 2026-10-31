# THROWAWAY shell proof — 2026-10-31

Verified on the workstation: the real Rust shell, software renderer and X11
under Xvfb. `xvfb/proof.txt` checks F2, arrows, fresh state after switching,
retaining the active surface after compile failure, and 1280x720.

The Pico branch was merged unchanged. `proof-pico.sh` loads the actual package
with `SHELL_SURFACES=pico,_test/a`. Screenshots in `pico/` show Celeste Classic,
Right to Hollow Knight, Left back to Celeste Classic, F2 to the blue test
surface and F2 back to fresh Pico. Four of the 12 fixture games have no art.
A temporary copy of the fixture with zero games exercises `stage.cart: null`.
Both rendered without property, image or QuickJS errors. No Pico files needed
shell-side repair.

| Xvfb Pico load | Compile ms | Logic-load ms | First completed frame ms |
| --- | ---: | ---: | ---: |
| Initial | 24.112 | 5.761 | 79.797 |
| After F2 back | 20.858 | 5.930 | 38.685 |
| Empty shelf (`cart: null`) | 24.601 | 6.056 | 76.252 |

The full `SHELL_STATS` lines are in `pico/stats.log` and `pico/empty-stats.log`.
The first-frame clock starts before runtime compilation. These are workstation
numbers, not SP numbers. Screenshot pixel comparisons and the logs were checked.

## Build and bundle

The final aarch64 build ran on fuji and passed its unit tests:
`/nix/store/l0sx6dpdwvp89j2b72ih72xvyha6xsgg-surface-shell-spike-0.0.0`.

```sh
nix build --impure --no-link --print-out-paths --expr \
  '(import <nixpkgs> { system = "aarch64-linux"; }).callPackage ./clients/surface-shell-spike/package.nix {}'

BUNDLE_DLOPEN= clients/zone-native-spike/device/bundle.sh \
  /nix/store/vh6dlmw3w5hp84px11cjpc11pbv7zf12-wtype-0.4 wtype /tmp/shell-wtype
SHELL_WTYPE_BUNDLE=/tmp/shell-wtype clients/surface-shell-spike/device/bundle.sh \
  /nix/store/l0sx6dpdwvp89j2b72ih72xvyha6xsgg-surface-shell-spike-0.0.0 \
  surface-shell-spike /tmp/surface-shell-sp
```

The archive `/tmp/surface-shell-sp.tar.gz` contains the loader, libraries, XKB,
fixture, test packages, Pico (including its font), and a prebuilt wtype companion.
The driver now validates every requested package's main.slint and logic.js
locally before it copies. Each run has a unique remote evidence directory.

## SP: first approved attempt stopped in preflight

The single approved invocation was:

```sh
clients/surface-shell-spike/device/drive.sh /tmp/surface-shell-sp.tar.gz _test/a,_test/b /tmp/surface-shell-sp-results
```

It copied the archive and runner to `/tmp`, then exited before stopping the
kiosk, extracting the bundle, or launching the shell: `korri-games.slice` did
not exist in cgroupfs. This was an overly strict preflight, not a running game.
The runner now treats a missing inactive game slice as empty, while retaining
the unknown-window and nonempty-game-slice refusals. This fix has passed
ShellCheck but needs a new approved device attempt for integration verification.

`device-preflight/` contains the captured process/tree checks. A read-only
60-second check from 03:21:06 to 03:22:06 UTC verified the kiosk stayed
active/running, MainPID 16682, Chromium PID 16689, NRestarts=0. No recovery was
needed. There are no SP switch timings or shell screenshots from this attempt.
A further device run needs a new approval; it has not been started.

## Checks

- `cargo fmt --check`: pass.
- `cargo clippy --release --all-targets -- -D warnings`: pass.
- `cargo test --release`: 10 pass (numbers, structs/models, null, PNG/SVG and file
  image caching, JS Map state/fresh contexts, input dedupe/repeat/axis mapping).
- `cargo build --release`: pass, x86_64.
- Nix aarch64 build and its unit tests: pass, fuji.
- ShellCheck on both proof scripts and all device scripts: pass.
- `nixfmt --check package.nix`: pass.
- Both real Xvfb proofs: pass; screenshots inspected.
- Pre-commit secret scanner: pass at the shell implementation commit.

Limits: runtime compilation pauses input/animations on the UI thread while the
old frame remains visible. Root is a spike-only input exception, not a product
permission design. The detailed numeric metadata API is Slint-internal and
pinned. No subjective feel or gap-free device switch has been verified yet.
