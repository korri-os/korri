#!/usr/bin/env bash
# THROWAWAY. Run after cargo build --release; no persistent service changes.
set -euo pipefail
bin=$(realpath "$1")
root=$(cd "$(dirname "$0")/.." && pwd)
out=$(realpath -m "$2")
mkdir -p "$out"
{ date -Is; uname -a; lscpu; free -h; } > "$out/machine.txt"
export SURFACE_VERIFY_JOBS=${SURFACE_VERIFY_JOBS:-8}
for n in 10 100 1000 5000; do
    "$bin" many "$root" "$n" > "$out/many-$n.jsonl" 2> "$out/many-$n.log"
done
"$bin" large "$root" "$out/fixtures" > "$out/large.jsonl" 2> "$out/large.log"
