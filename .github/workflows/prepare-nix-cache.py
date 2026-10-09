#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 nix
"""Stop unfinished builds before archiving a disposable Actions runner's store."""

import os
from pathlib import Path
import shutil
import subprocess


def build_remnants(store, registered):
    # Interrupted builds leave unreadable locks and sandbox directories. Never
    # remove a registered output, even if its name has one of these suffixes.
    return [
        path
        for path in store.iterdir()
        if path.name.endswith((".lock", ".drv.chroot")) and str(path) not in registered
    ]


def main():
    if (
        os.environ.get("GITHUB_ACTIONS") != "true"
        or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"
    ):
        raise SystemExit("This cleanup runs only on a disposable GitHub-hosted runner.")

    nix_store = shutil.which("nix-store")
    if nix_store is None:
        raise SystemExit("Nix must be installed before preparing its cache.")

    # Stopping only the daemon can leave its workers alive. Kill the service's
    # whole control group, with socket activation disabled, before reading DB.
    subprocess.run(["sudo", "systemctl", "stop", "nix-daemon.socket"], check=True)
    subprocess.run(
        [
            "sudo",
            "systemctl",
            "kill",
            "--kill-who=all",
            "--signal=SIGKILL",
            "nix-daemon.service",
        ],
        check=True,
    )
    subprocess.run(["sudo", "systemctl", "stop", "nix-daemon.service"], check=True)
    try:
        registered = set(
            subprocess.check_output(
                ["sudo", nix_store, "--store", "local", "--query", "--all"], text=True
            ).splitlines()
        )
        remnants = build_remnants(Path("/nix/store"), registered)
        for offset in range(0, len(remnants), 100):
            subprocess.run(
                ["sudo", "rm", "-rf", "--", *map(str, remnants[offset : offset + 100])],
                check=True,
            )
        print(
            f"Removed {len(remnants)} unregistered build remnants; kept {len(registered)} registered paths."
        )
    finally:
        # The cache action queries Nix and checkpoints its DB. Start a fresh,
        # idle daemon instead of archiving alongside the interrupted workers.
        subprocess.run(["sudo", "systemctl", "start", "nix-daemon.socket"], check=True)
    subprocess.run(["df", "-h", "/nix"], check=True)


if __name__ == "__main__":
    main()
