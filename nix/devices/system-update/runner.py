#!/usr/bin/env python3
"""Shared SSH, signing and prebuilt-system staging for device boot adapters."""

import argparse
import base64
from collections.abc import Callable
from dataclasses import dataclass
import hashlib
import os
from pathlib import Path
import re
import shlex
import socket
import stat
import subprocess
import tempfile

SSH_OPTIONS = [
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=10",
    "-o",
    "ServerAliveInterval=5",
    "-o",
    "ServerAliveCountMax=3",
    "-o",
    "StrictHostKeyChecking=yes",
    "-p",
    "2222",
]
ZAO_KEY = "korri-cache-zao-1:thKjQnMnPl8AqTuZWJTn+ej9BORTPqzleGue4ZfJ2u4="


@dataclass(frozen=True)
class UpdateDevice:
    """Boot adapter contract; fields come from each existing device configuration."""

    description: str
    HERE: Path
    ROOT: Path
    DEVICE: str
    TARGET: str
    SYSTEM: str
    CONFIG_NAME: str
    prepare: Callable[[Path, Path, str, str], tuple[bytes, int]]
    BUILD_OPTIONS: tuple[str, ...] = ()


def require(condition: object, message: str) -> None:
    if not condition:
        raise ValueError(message)


def remote(
    device: UpdateDevice, action: str, *args: str, archive: bytes | None = None
) -> str:
    source = base64.b64encode(
        (
            Path(__file__).with_name("target.sh").read_bytes()
            + b"\n"
            + (device.HERE / "target.sh").read_bytes()
        )
    ).decode()
    command = 'bash -c "$(printf %s ' + shlex.quote(source) + ' | base64 -d)" -- '
    command += shlex.join([action, *args])
    result = subprocess.run(
        ["ssh", *SSH_OPTIONS, device.TARGET, command],
        input=archive,
        stdout=subprocess.PIPE,
        check=False,
    )
    require(
        result.returncode == 0,
        f"remote {action} failed (exit {result.returncode}); see SSH stderr",
    )
    return result.stdout.decode()


def display(output: str) -> None:
    # Transport the exact config for checksum/race checks, not as user output.
    print(
        "\n".join(
            line
            for line in output.splitlines()
            if not line.startswith(("grub_base64=", "extlinux_base64="))
        )
    )


def inspect(device: UpdateDevice) -> tuple[str, dict[str, str], str]:
    output = remote(device, "status")
    name = device.CONFIG_NAME
    fields = dict(line.split("=", 1) for line in output.splitlines() if "=" in line)
    for key in ("current", "profile", name + "_sha256", name + "_base64"):
        require(key in fields, "missing status field: " + key)
    require(re.fullmatch(device.SYSTEM, fields["current"]), "unexpected booted system")
    text = base64.b64decode(fields[name + "_base64"], validate=True).decode()
    require(
        hashlib.sha256(text.encode()).hexdigest() == fields[name + "_sha256"],
        "boot config status checksum mismatch",
    )
    return output, fields, text


def build(device: UpdateDevice, attribute: str, out_link: Path) -> Path:
    return Path(
        subprocess.check_output(
            [
                "nix",
                "build",
                *device.BUILD_OPTIONS,
                "--out-link",
                str(out_link),
                "--print-out-paths",
                ".#" + attribute,
            ],
            cwd=device.ROOT,
            text=True,
        ).strip()
    )


def main(device: UpdateDevice) -> None:
    name = device.CONFIG_NAME
    parser = argparse.ArgumentParser(description=device.description)
    actions = parser.add_mutually_exclusive_group()
    actions.add_argument(
        "--status",
        action="store_true",
        help="read mount identity, booted/profile systems and boot selection",
    )
    actions.add_argument(
        "--rollback",
        action="store_true",
        help="restore the on-device previous config and profile; does not reboot",
    )
    parser.add_argument(
        "--expected-system",
        help="approved current /run/current-system path; required for writes",
    )
    args = parser.parse_args()
    if not args.status:
        require(
            args.expected_system and re.fullmatch(device.SYSTEM, args.expected_system),
            "writes require --expected-system STORE_PATH",
        )
    output, before, previous = inspect(device)
    if args.status:
        display(output)
        return
    require(
        before["current"] == args.expected_system,
        "booted system does not match --expected-system",
    )
    if args.rollback:
        display(remote(device, "rollback", before["current"], before[name + "_sha256"]))
        return
    require(
        socket.gethostname().split(".")[0] == "zao", "sign and deploy from Zao only"
    )
    require(
        before["profile"] == before["current"],
        "profile differs from booted system; reboot or recover first",
    )
    key = Path.home() / ".config/korri/cache-key.secret"
    info = key.stat()
    require(
        stat.S_ISREG(info.st_mode)
        and info.st_uid == os.getuid()
        and not info.st_mode & 0o077,
        "signing key must be private and owned by this user",
    )
    with key.open("rb") as secret:
        public = subprocess.check_output(
            ["nix", "key", "convert-secret-to-public"],
            stdin=secret,
            stderr=subprocess.DEVNULL,
            text=True,
        ).strip()
    require(public == ZAO_KEY, "unexpected signing identity")
    with tempfile.TemporaryDirectory(prefix=device.DEVICE + "-update-") as temp:
        # These links keep both results alive until staging has completed.
        system = build(
            device,
            "nixosConfigurations." + device.DEVICE + ".config.system.build.toplevel",
            Path(temp) / "system",
        )
        archive, required = device.prepare(
            system, Path(temp), previous, before["current"]
        )
        # A fresh read catches another worker changing boot state during builds.
        _, now, _ = inspect(device)
        require(now == before, "device changed during build; inspect again")
        subprocess.run(
            [
                "nix",
                "store",
                "sign",
                "--recursive",
                "--key-file",
                str(key),
                str(system),
            ],
            check=True,
        )
        # The remote store retains require-sigs. Neither this copy nor the SSH
        # script evaluates a flake, realises a derivation or dispatches a build.
        subprocess.run(
            ["nix", "copy", "--to", "ssh-ng://" + device.TARGET, str(system)],
            check=True,
            env={**os.environ, "NIX_SSHOPTS": shlex.join(SSH_OPTIONS)},
        )
        display(
            remote(
                device,
                "update",
                before["current"],
                before[name + "_sha256"],
                str(system),
                str(required),
                archive=archive,
            )
        )
        print("New system:", system)
