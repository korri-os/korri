#!/usr/bin/env nix
#! nix shell nixpkgs#python3 --command python3
"""Run korrid's ignored, effect-free production argument computation on this host."""

import argparse
import json
import os
from pathlib import Path
import subprocess

TEST = "linux_viewer::production::tests::emit_the_installed_handheld_proof_command"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--test-binary", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--compatible", type=Path, required=True)
    parser.add_argument("--rendered", type=Path, required=True)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--keydir", required=True)
    parser.add_argument("--address", required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    environment = {
        **os.environ,
        "KORRI_PROOF_MANIFEST": args.manifest.read_text(),
        "KORRI_PROOF_COMPATIBLE_FILE": str(args.compatible.resolve()),
        "KORRI_PROOF_RENDERED_ARGS": args.rendered.read_text(),
        "KORRI_PROOF_INPUT_NODES": args.inputs.read_text(),
        "KORRI_PROOF_KEYDIR": args.keydir,
        "KORRI_PROOF_ADDRESS": args.address,
    }
    result = subprocess.run(
        [
            str(args.test_binary.resolve()),
            "--exact",
            TEST,
            "--ignored",
            "--nocapture",
        ],
        env=environment,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    args.out.with_suffix(".log").write_text(result.stdout)
    if result.returncode:
        raise SystemExit(
            f"argument computation failed; see {args.out.with_suffix('.log')}"
        )
    commands = [
        json.loads(line.removeprefix("KORRI_PROOF_COMMAND="))
        for line in result.stdout.splitlines()
        if line.startswith("KORRI_PROOF_COMMAND=")
    ]
    if len(commands) != 1 or not all(isinstance(value, str) for value in commands[0]):
        raise SystemExit("korrid did not emit exactly one string-vector command")
    args.out.write_text(json.dumps(commands[0], indent=2) + "\n")
    print(args.out)


if __name__ == "__main__":
    main()
