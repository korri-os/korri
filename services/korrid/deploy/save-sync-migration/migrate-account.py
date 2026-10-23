#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3
"""Offline default-account move or explicitly consented stranded-switch repair."""

import sys

sys.dont_write_bytecode = True
from migration import run

if __name__ == "__main__":
    sys.exit(run("account"))
