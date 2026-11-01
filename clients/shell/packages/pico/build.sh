#!/usr/bin/env bash
# Bundle logic.ts to logic.js, as the shell loads it (contract 3.1).
set -euo pipefail
cd "$(dirname "$0")"
# Minified: QuickJS parses it on every surface load, inside the 50 ms budget.
bun build logic.ts --outfile logic.js --target browser --format iife --minify
