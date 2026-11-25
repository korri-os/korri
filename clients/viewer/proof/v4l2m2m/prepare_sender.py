#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p 'python3.withPackages (p: [ p.numpy p.pillow ])' openssh nix
"""Build the existing sender's tools on this build host; copy them to aka.

This starts no compositor, Sunshine or viewer. The actual proof must use
these private fixture tools, never the product Sunshine.
"""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[4]
os.environ["KORRI_ROOT"] = str(ROOT)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sender", default="root@aka")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    os.environ["CADENCE_NIXPKGS"] = subprocess.check_output(
        [
            "nix",
            "eval",
            "--raw",
            "--impure",
            "--expr",
            f'(builtins.getFlake "{ROOT}").inputs.nixpkgs.outPath',
        ],
        text=True,
    )
    source = ROOT / "clients/viewer/android/proof/run.py"
    spec = importlib.util.spec_from_file_location("existing_viewer_proof", source)
    proof = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(proof)
    command, parts = proof.sender_tools(args.sender, False, "auto")
    args.out.write_text(
        json.dumps({"command": command, "parts": parts}, indent=2) + "\n"
    )


if __name__ == "__main__":
    main()
