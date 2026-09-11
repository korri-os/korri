#!/usr/bin/env nix
#! nix shell nixpkgs#bash nixpkgs#python3 --command bash
# Build machine only. Requires Bun 1.3.11, host Rust/C/libclang (Oxc + bindgen).
# Intentionally does NOT repeat the old 50-run matrix or perform a cap sweep.
set -euo pipefail
here=$(cd -- "$(dirname -- "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
scratch=${PROBE_SCRATCH:-$(mktemp -d /tmp/retroarch-effect-quickjs.XXXXXX)}
logs=${PROBE_LOGS:-$scratch/logs}
heap=${PROBE_HEAP_MIB:-16}
deadline=${PROBE_DEADLINE_MS:-250}
samples=${PROBE_SAMPLES:-1}
[[ $samples =~ ^[1-9][0-9]*$ ]] || { echo 'PROBE_SAMPLES must be a positive integer' >&2; exit 2; }
mkdir -p "$scratch/src" "$logs"
[[ -z $(find "$logs" -mindepth 1 -maxdepth 1 -print -quit) ]] || { echo 'Use a fresh log directory' >&2; exit 1; }
[[ $(bun --version) == 1.3.11 ]] || { echo 'Use measured Bun 1.3.11' >&2; exit 1; }
{
  uname -a
  lscpu
  rustc --version
  cargo --version
  bun --version
  cc --version
  printf 'LIBCLANG_PATH=%s\nBINDGEN_EXTRA_CLANG_ARGS=%s\n' "${LIBCLANG_PATH:-}" "${BINDGEN_EXTRA_CLANG_ARGS:-}"
} > "$logs/environment.txt"
(cd "$root/plugins/retroarch" && bun install --frozen-lockfile --ignore-scripts) > "$logs/install.txt" 2>&1
(cd "$here" && bun install --frozen-lockfile --ignore-scripts) > "$logs/platform-install.txt" 2>&1
cp "$here/Cargo.toml" "$here/Cargo.lock" "$scratch/"
cp "$here/src/"*.rs "$here/src/"*.js "$scratch/src/"
python3 "$here/check-cargo-lock.py" "$root/services/korrid/Cargo.lock" "$scratch/Cargo.lock"
cargo build --release --locked --manifest-path "$scratch/Cargo.toml" > "$logs/build.txt" 2>&1
probe="$scratch/target/release/retroarch-effect-quickjs-probe"
"$probe" cases > "$scratch/cases.json"
bun "$here/control.ts" "$scratch/cases.json" > "$logs/bun-control.jsonl"
# Preserve the empty-platform baseline; add no globals on this path.
bun "$here/prepare.ts" schema-minify "$scratch/none.json" none > "$logs/none-preparation.jsonl"
"$probe" "$scratch/none.json" "$heap" "$deadline" "$samples" > "$logs/none-baseline.jsonl"
for mode in schema schema-minify; do
  bun "$here/prepare.ts" "$mode" "$scratch/$mode-real.json" real > "$logs/$mode-preparation.jsonl"
  "$probe" "$scratch/$mode-real.json" "$heap" "$deadline" 1 > "$logs/$mode-zero-timer.jsonl"
  "$probe" "$scratch/$mode-real.json" "$heap" "$deadline" "$samples" policy timers > "$logs/$mode-timers.jsonl"
done
# Independent runtime: check real primitives even when policy import fails.
"$probe" "$scratch/schema-minify-real.json" "$heap" "$deadline" 1 primitives timers > "$logs/primitives.jsonl"
for mode in timer-checks timer-deadline timer-nested-deadline timer-interrupt timer-throw timer-cleanup; do
  "$probe" "$scratch/schema-minify-real.json" "$heap" "$deadline" "$samples" "$mode" timers > "$logs/$mode.jsonl"
done
python3 "$here/check-results.py" "$logs" "$samples" | tee "$logs/assertions.txt"
printf 'Scratch: %s\nLogs: %s\n' "$scratch" "$logs"
