#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash git
# shellcheck shell=bash
set -euo pipefail
unset shellHook
cd "$KORRI_ROOT"
python3 clients/cli/tests/parity.py
cargo fmt --manifest-path clients/cli/Cargo.toml --check
cargo clippy --manifest-path clients/cli/Cargo.toml --all-targets --no-deps -- -D warnings
cargo test --manifest-path clients/cli/Cargo.toml "$@"
