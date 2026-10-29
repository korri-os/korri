#!/usr/bin/env nix
#! nix shell nixpkgs#bash nixpkgs#coreutils nixpkgs#gnutar nixpkgs#gzip nixpkgs#gnused --command bash
# shellcheck shell=bash
# Workstation only. Extract a verified prebuilt SDK; never contact a device.
set -euo pipefail
archive="${1:?usage: webos-sdk.sh ARCHIVE DESTINATION}"
destination="${2:?usage: webos-sdk.sh ARCHIVE DESTINATION}"
expected=2230a6351e5cce6fcb9d4fdcb4697335f223995401ef969c81744aaeb14c5344
printf '%s  %s\n' "$expected" "$archive" | sha256sum --check
if [[ -e "$destination" ]]; then
  echo "Destination already exists; use the prepared SDK or choose a new directory: $destination" >&2
  exit 1
fi
mkdir -p "$destination"
tar xzf "$archive" --strip-components=1 -C "$destination"
cd "$destination"
bash ./relocate-sdk.sh
printf 'SDK extracted and relocated: %s\n' "$PWD"
echo 'Run the webOS probe app next. It prepares the workstation ELF loader before executing SDK tools.'
