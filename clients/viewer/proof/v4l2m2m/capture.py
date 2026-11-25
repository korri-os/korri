#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#openssh nixpkgs#nix --command python3
"""Capture the existing sway output; optionally wake it and restore its power."""

import argparse
import json
from pathlib import Path
import shlex
import subprocess

ROOT = Path(__file__).resolve().parents[4]
REMOTE = r"""
set -eu
mode=$1
grim=$2
shift 2
control() {
  for ctl in /nix/store/*-sway-*/bin/swaymsg; do
    if [ -x "$ctl" ]; then
      runuser -u korri -- env SWAYSOCK=/run/korri-compositor/sway-ipc.sock "$ctl" "$@"
      return
    fi
  done
  echo 'no installed swaymsg' >&2
  return 1
}
case "$mode" in
  inspect) control -t get_outputs -r ;;
  power) control output "$1" power "$2" ;;
  capture)
    # Exact prebuilt output only. The device cannot build or dispatch builds.
    nix build --no-link --option max-jobs 0 --option builders '' \
      --option fallback false --option require-sigs true "$grim" >&2
    exec runuser -u korri -- env XDG_RUNTIME_DIR=/run/user/1000 \
      WAYLAND_DISPLAY=korri-wayland "$grim/bin/grim" -t png - ;;
  *) exit 2 ;;
esac
"""


def remote(
    device: str, grim: str, mode: str, *values: str
) -> subprocess.CompletedProcess:
    return subprocess.run(
        [
            "ssh",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=15",
            "-p",
            "2222",
            f"root@{device}.local",
            shlex.join(["bash", "-s", "--", mode, grim, *values]),
        ],
        input=REMOTE.encode(),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("device", choices=["tabs6", "op6"])
    parser.add_argument("out", type=Path)
    parser.add_argument(
        "--wake",
        action="store_true",
        help="Restore initially sleeping outputs after capture",
    )
    args = parser.parse_args()
    # This build host evaluates. The device receives no flake or derivation.
    grim = subprocess.check_output(
        [
            "nix",
            "eval",
            "--raw",
            "--impure",
            "--expr",
            f'(builtins.getFlake "{ROOT}").inputs.nixpkgs.legacyPackages.aarch64-linux.grim.outPath',
        ],
        text=True,
    )
    before = json.loads(remote(args.device, grim, "inspect").stdout)
    sleeping = [
        output["name"]
        for output in before
        if output.get("active") and not output.get("power")
    ]
    woken = []
    try:
        if args.wake:
            for name in sleeping:
                remote(args.device, grim, "power", name, "on")
                woken.append(name)
        result = remote(args.device, grim, "capture")
        args.out.with_suffix(".log").write_bytes(result.stderr)
        if not result.stdout.startswith(b"\x89PNG\r\n\x1a\n"):
            raise RuntimeError("grim did not return a PNG")
        args.out.write_bytes(result.stdout)
        print(args.out)
    except subprocess.CalledProcessError as error:
        args.out.with_suffix(".log").write_bytes(error.stderr)
        raise SystemExit(
            f"capture failed; see {args.out.with_suffix('.log')}"
        ) from error
    finally:
        for name in woken:
            remote(args.device, grim, "power", name, "off")
        after = json.loads(remote(args.device, grim, "inspect").stdout)
        args.out.with_suffix(".outputs.json").write_text(
            json.dumps({"before": before, "after": after}, indent=2) + "\n"
        )


if __name__ == "__main__":
    main()
