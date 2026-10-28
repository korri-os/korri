#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash git
# shellcheck shell=bash
set -euo pipefail
export KORRI_ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
exec nix develop "$KORRI_ROOT#korrid" --command nix shell --inputs-from "$KORRI_ROOT" nixpkgs#typescript --command bash "$KORRI_ROOT/clients/cli/check-in-shell.sh" "$@"
