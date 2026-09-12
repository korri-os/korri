#!/usr/bin/env bash
# Nix post-build hook: sign every locally built path into a staging cache.
#
# Nix runs this after each build with $OUT_PATHS set. It must be fast and it
# must not fail a build, so it only writes to a local file cache. Uploading
# to GitHub is a separate batch job: release assets are a poor fit for a
# per-path synchronous push, and a network stall here would stall the build.
#
# Required environment:
#   KORRI_CACHE_SECRET_KEY  this builder's signing key
#   KORRI_CACHE_STAGING     directory for the staging file cache
set -euo pipefail

key=${KORRI_CACHE_SECRET_KEY:?post-build hook needs a signing key}
staging=${KORRI_CACHE_STAGING:?post-build hook needs a staging directory}

[ -n "${OUT_PATHS:-}" ] || exit 0
[ -f "$key" ] || { echo "korri-cache: signing key missing: $key" >&2; exit 0; }

mkdir -p "$staging"

# shellcheck disable=SC2086
exec nix copy --to "file://$staging?secret-key=$key" $OUT_PATHS
