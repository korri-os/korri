#!/usr/bin/env nix-shell
#!nix-shell -i bash -p python3
# PROTOTYPE — serve the boot-splash prototype on every interface.
# Serves the repo root so /brand/*.svg resolves; the page is /prototypes/boot-splash/.
cd "$(dirname "$0")/../.." && exec python3 -m http.server --bind 0.0.0.0 "${PORT:-8765}"
