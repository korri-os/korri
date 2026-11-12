#!/usr/bin/env bash
# The shell's probe (examples/surface-probe.rs walk) over every Velvet
# scenario: each model of each preview/scenarios/*.json ({name, models,
# steps}) is written as its own SurfaceModel and walked at the probe's five
# sizes, up to MAX states, several models at once. The probe presses every
# button from every state it reaches, judges where each press goes against
# what is drawn (contract 8.4) and taps every target (ADR 0055). Prints each
# walk's line and every FAIL, then the total; exits 1 when any walk fails.
#
#   bash packages/velvet/preview/probe.sh PROBE [MAX] [JOBS] [SCENARIOS...]
#
# PROBE: the built surface-probe (cargo build --release --features fixture
# --example surface-probe, in clients/shell). MAX defaults to 12, JOBS to 8.
# With no SCENARIOS it walks every one, which takes hours: the probe draws
# and taps every target of every state at five sizes, and most scenarios
# share a model. Name the ones whose models differ for a run of minutes
# (README, "Checks").
set -euo pipefail
probe=$(realpath "$1")
max=${2:-12}
jobs=${3:-8}
shift $(( $# < 3 ? $# : 3 ))
velvet=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/models" "$work/out"
if (( $# > 0 )); then scenarios=(); for name in "$@"; do scenarios+=("$velvet/preview/scenarios/$name.json"); done; else scenarios=("$velvet"/preview/scenarios/*.json); fi
for scenario in "${scenarios[@]}"; do
  name=$(basename "$scenario" .json)
  count=$(jq '.models | length' "$scenario")
  for ((i = 0; i < count; i++)); do
    jq ".models[$i]" "$scenario" > "$work/models/$name.$i.json"
  done
done
export probe max velvet work
find "$work/models" -name '*.json' -print0 | sort -z |
  xargs -0 -P "$jobs" -I{} bash -c '
    name=$(basename "$1" .json)
    "$probe" walk "$velvet" --model "$1" --max "$max" --out "$work/out/img" > "$work/out/$name.txt" 2>&1 || true
  ' _ {}
cat "$work"/out/*.txt | grep -E ' states, |^FAIL' || true
walks=$(cat "$work"/out/*.txt | grep -c ' states, ' || true)
failures=$(cat "$work"/out/*.txt | grep -c '^FAIL' || true)
models=$(find "$work/models" -name '*.json' | wc -l)
echo "velvet probe: $models models, $walks walks, $failures failures"
[[ $walks -eq $((models * 5)) && $failures -eq 0 ]]
