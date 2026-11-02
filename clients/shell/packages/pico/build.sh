#!/usr/bin/env bash
# Bundle logic.ts to logic.js, as the shell loads it (contract 3.1), with the
# flake's bun: `nix run .#pico-check` rebuilds it and fails when it differs.
set -euo pipefail
cd "$(dirname "$0")/../.."
bun run pico:build
