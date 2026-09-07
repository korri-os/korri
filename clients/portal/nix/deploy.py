#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#nix nixpkgs#openssh --command python3
"""Update the portal's Nix profile without switching the device's NixOS system."""

import argparse
import os
import pathlib
import re
import shlex
import subprocess
import tempfile


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "target", help="SSH target with permission to select the portal profile"
    )
    parser.add_argument("--ssh-config", type=pathlib.Path)
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--rollback", action="store_true")
    action.add_argument("--status", action="store_true")
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_.@-]+", args.target) or args.target.startswith(
        "-"
    ):
        parser.error("Use an SSH host or user@host target, not a URL or SSH options")
    ssh_options = []
    if args.ssh_config:
        ssh_options = ["-F", str(args.ssh_config.resolve())]
    environment = os.environ.copy()
    if ssh_options:
        environment["NIX_SSHOPTS"] = shlex.join(ssh_options)
    ssh = ["ssh", *ssh_options, "--", args.target]
    selector = "/run/current-system/sw/bin/korri-portal-select"
    subprocess.run([*ssh, "test", "-x", selector], check=True)
    if args.rollback or args.status:
        subprocess.run(
            [*ssh, selector, "rollback" if args.rollback else "status"], check=True
        )
        return
    root = pathlib.Path(os.environ["KORRI_ROOT"])
    # Keep a GC root until the receiving device has selected the bundle.
    with tempfile.TemporaryDirectory(prefix="korri-portal-deploy-") as temporary:
        output = subprocess.check_output(
            [
                "nix",
                "build",
                "--out-link",
                f"{temporary}/result",
                "--print-out-paths",
                ".#korri-portal",
            ],
            cwd=root,
            text=True,
        ).strip()
        if not re.fullmatch(r"/nix/store/[a-z0-9]{32}-[^/\s]+", output):
            raise RuntimeError(
                f"Expected one immutable portal output, received {output!r}"
            )
        # Explicitly trust the operator's local build over SSH for this copy,
        # not arbitrary caches or a persistent daemon setting.
        subprocess.run(
            [
                "nix",
                "copy",
                "--no-check-sigs",
                "--to",
                f"ssh-ng://{args.target}",
                output,
            ],
            env=environment,
            check=True,
        )
        subprocess.run([*ssh, selector, "switch", output], check=True)


if __name__ == "__main__":
    main()
