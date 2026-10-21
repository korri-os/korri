#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p "python3.withPackages (p: [ p.pyyaml ])"
"""Move catalog-matched RetroArch files into each game's save folder, once."""

import sys

sys.dont_write_bytecode = True
from migration import run

if __name__ == "__main__":
    sys.exit(run("retroarch"))
