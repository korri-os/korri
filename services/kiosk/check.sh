#!/usr/bin/env nix-shell
#! nix-shell -i bash -p cargo rustc gcc rustfmt clippy chromium nixfmt
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
cargo fmt --check
nixfmt --check package.nix
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
if [[ "${1:-}" == --chromium ]]; then
  export KORRI_TEST_CHROMIUM
  KORRI_TEST_CHROMIUM="$(command -v chromium)"
  cargo test --locked --test chromium -- --ignored --nocapture
fi
