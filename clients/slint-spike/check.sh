#!/usr/bin/env bash
# THROWAWAY SPIKE check. Run through `nix run .#slint-spike-check`, which sets
# KORRI_ROOT, CARGO_TARGET_DIR, the toolchain and the runtime libraries.
set -euo pipefail
crate="$KORRI_ROOT/clients/slint-spike"
manifest="$crate/Cargo.toml"
spike="$CARGO_TARGET_DIR/debug/korri-slint-spike"

# The spike's scratch directories go under a root this check owns, so the
# leak assertion below sees only this run's directories.
scratch_root="$(mktemp -d)"
audit="$(mktemp -d)"
trap 'rm -rf "$scratch_root" "$audit"' EXIT

echo "== fmt"
cargo fmt --manifest-path "$manifest" --check
echo "== clippy"
cargo clippy --locked --manifest-path "$manifest" --all-targets -- -D warnings
echo "== unit tests (state mapping, layout plan)"
cargo test --locked --manifest-path "$manifest"
echo "== build"
cargo build --locked --manifest-path "$manifest"

echo "== refuses inherited peer configuration"
set +e
refusal="$(TMPDIR="$scratch_root" KORRID_UPSTREAMS='[]' "$spike" --self-test 2>&1)"
status=$?
set -e
if [[ $status -ne 2 || "$refusal" != *"KORRID_UPSTREAMS"* || "$refusal" == *"[]"* ]]; then
  echo "FAIL: inherited KORRID_UPSTREAMS was not refused by name only" >&2
  echo "$refusal" >&2
  exit 1
fi
echo "  ok   KORRID_UPSTREAMS refused before korrid starts (value not echoed)"

echo "== Xvfb self-test: real window, Refresh callback, real korrid, auth probes"
# Address-only audit: trace connect(2) alone, with no string bodies (-s 0).
# connect carries a socket address, never a request body, so the bearer
# cannot enter the trace. The trace goes straight into awk; only aggregate
# counts are written, never a port, a path or a raw trace line.
summarize='
  { family = "other" }
  /sa_family=AF_INET6/ { family = ($0 ~ /"::1"/) ? "AF_INET6 loopback" : "AF_INET6 OUTSIDE" }
  /sa_family=AF_INET[,}]/ { family = ($0 ~ /inet_addr\("127\.0\.0\.1"\)/) ? "AF_INET loopback" : "AF_INET OUTSIDE" }
  /sa_family=AF_UNIX/ { family = "AF_UNIX" }
  /sa_family=AF_NETLINK/ { family = "AF_NETLINK" }
  /connect\(/ { count[family]++ }
  END { for (f in count) print f, count[f] }'
TMPDIR="$scratch_root" xvfb-run -a -s "-screen 0 1280x800x24 -nolisten tcp" \
  strace -f -qq -s 0 -e trace=connect -o "|awk '$summarize' > '$audit/families'" \
  "$spike" --self-test | tee "$audit/run.log"
grep -q '^SELF-TEST PASS$' "$audit/run.log"

echo "== network audit (connect(2) socket families, counts only)"
sort "$audit/families" | sed 's/^/  /'
if grep -q OUTSIDE "$audit/families"; then
  echo "FAIL: connect(2) to an IP address outside loopback" >&2
  exit 1
fi
echo "  ok   every IP connect(2) went to 127.0.0.1 or ::1"

echo "== scratch storage removed"
if compgen -G "$scratch_root/korri-slint-spike-*" >/dev/null; then
  echo "FAIL: this run's scratch directories remain" >&2
  exit 1
fi
echo "  ok   no korri-slint-spike-* directory remains"
echo "SLINT SPIKE CHECK PASS"
