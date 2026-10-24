#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash git
# shellcheck shell=bash
set -euo pipefail
unset shellHook
cd "$KORRI_ROOT/services/korrid"
cargo fmt --check
cargo clippy --lib --bin korrid --no-deps
cargo test --lib cli::tests
cargo test --lib cli_forwarding
cargo test --lib cli_socket_excludes
cargo test --lib upstream_native::tests
cargo test --lib game_and_modifier_sandboxes -- --nocapture
cargo test --lib authorization::tests
cargo test --lib status_identity_tests
cargo test --lib controller_count_tests
cargo test --bin korrid
cargo test --test daemon_arguments
