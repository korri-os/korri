#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash git nix
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
export CARGO_TARGET_DIR="$ROOT/.cache/inputd-target"

cd "$ROOT/services/inputd"
cargo fmt --check
cargo fmt --manifest-path core/Cargo.toml --check
cargo clippy --all-targets -- -D warnings
cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings
cargo test --all-targets
cargo build --bin korri-ledger-proof
cargo test --manifest-path core/Cargo.toml --all-targets
cargo check --manifest-path core/Cargo.toml --target wasm32-unknown-unknown

cd "$ROOT"
bash -n services/inputd/deploy/device-check.sh services/inputd/deploy/test-device-check.sh \
  services/inputd/deploy/test-device-bitmap.sh
shellcheck services/inputd/deploy/device-check.sh services/inputd/deploy/test-device-check.sh \
  services/inputd/deploy/test-device-bitmap.sh
services/inputd/deploy/test-device-bitmap.sh
services/inputd/deploy/test-device-check.sh

nix build --no-link \
  .#checks."$(nix eval --raw --impure --expr builtins.currentSystem)".inputplumber-korri-package \
  .#checks."$(nix eval --raw --impure --expr builtins.currentSystem)".korri-input-module \
  .#checks."$(nix eval --raw --impure --expr builtins.currentSystem)".korrid-linux-device-module \
  .#checks."$(nix eval --raw --impure --expr builtins.currentSystem)".korri-linux-host-module \
  .#checks."$(nix eval --raw --impure --expr builtins.currentSystem)".sunshine-korri-package \
  .#checks."$(nix eval --raw --impure --expr builtins.currentSystem)".sunshine-korri-runtime-settings \
  .#checks."$(nix eval --raw --impure --expr builtins.currentSystem)".sunshine-korri-input-seat-patch \
  .#checks."$(nix eval --raw --impure --expr builtins.currentSystem)".sunshine-korri-certificate-control \
  .#checks."$(nix eval --raw --impure --expr builtins.currentSystem)".korri-input-seat-receiver \
  .#checks."$(nix eval --raw --impure --expr builtins.currentSystem)".korri-inputd-package
