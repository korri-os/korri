#!/usr/bin/env python3
"""Build, sign and stage an internal-UFS Mini V2 system from Zao. Never reboot."""

import argparse
import base64
import hashlib
import io
import os
from pathlib import Path
import re
import shlex
import socket
import stat
import subprocess
import tarfile
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = Path(os.environ.get("KORRI_ROOT", HERE.parents[3]))
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
TARGET = "root@10.42.3.1"
SYSTEM = r"/nix/store/[0-9abcdfghijklmnpqrsvwxyz]{32}-nixos-system-rpminiv2-sd-card-[A-Za-z0-9+._=-]+"
PAYLOAD = r"/EFI/nixos/[0-9abcdfghijklmnpqrsvwxyz]{32}-[A-Za-z0-9+._=-]+"
ZAO_KEY = "korri-cache-zao-1:thKjQnMnPl8AqTuZWJTn+ej9BORTPqzleGue4ZfJ2u4="


def require(condition, message):
    if not condition:
        raise ValueError(message)


def parse_grub(text):
    """Read only the existing, constrained GRUB dialect. Never evaluate GRUB."""
    require(len(text.encode()) <= 64 * 1024, "GRUB config exceeds 64 KiB")
    matches = list(re.finditer(r"^menuentry '[^'\n{}]+' \{\n[^{}]*^\}\n?", text, re.M))
    require(matches, "no complete GRUB menu entries")
    header = text[: matches[0].start()]
    entries = []
    end = matches[0].start()
    for match in matches:
        require(
            not text[end : match.start()].strip(),
            "unexpected text between menu entries",
        )
        fields = {}
        for line in match[0].splitlines()[1:-1]:
            words = line.split(None, 1)
            require(
                len(words) == 2 and words[0] not in fields, "invalid menu directive"
            )
            fields[words[0]] = words[1]
        require(
            set(fields) == {"search", "linux", "initrd", "devicetree"},
            "unexpected menu directives",
        )
        linux = fields["linux"].split()
        require(len(linux) >= 2, "missing kernel parameters")
        paths = [linux[0], fields["initrd"], fields["devicetree"]]
        require(
            all(re.fullmatch(PAYLOAD, path) for path in paths),
            "unsafe boot payload path",
        )
        require(
            fields["search"] == "--set -f " + linux[0], "search and kernel disagree"
        )
        init = [param[5:] for param in linux[1:] if param.startswith("init=")]
        require(
            len(init) == 1 and init[0].endswith("/init"), "expected one system init"
        )
        system = init[0][:-5]
        require(re.fullmatch(SYSTEM, system), "unsafe Mini V2 system path")
        entries.append({"text": match[0], "system": system, "paths": paths})
        end = match.end()
    require(not text[end:].strip(), "unexpected trailing GRUB text")
    return header, entries


def updated_grub(new, previous, current):
    header, entries = parse_grub(new)
    old_header, old_entries = parse_grub(previous)
    require(len(entries) == 1, "shared firmware must have one entry")
    require(
        header.split() == old_header.split(),
        "installed GRUB handoff differs from image",
    )
    require(
        old_entries[0]["system"] == current, "GRUB default differs from booted system"
    )
    require(entries[0]["system"] != current, "already running this system")
    fallback = old_entries[0]["text"].replace("' {", " (previous)' {", 1)
    return header + entries[0]["text"] + "\n" + fallback


def remote(action, *args, archive=None):
    source = base64.b64encode((HERE / "target.sh").read_bytes()).decode()
    command = 'bash -c "$(printf %s ' + shlex.quote(source) + ' | base64 -d)" -- '
    command += shlex.join([action, *args])
    result = subprocess.run(
        ["ssh", *SSH_OPTIONS, TARGET, command],
        input=archive,
        stdout=subprocess.PIPE,
        check=False,
    )
    require(
        result.returncode == 0,
        f"remote {action} failed (exit {result.returncode}); see SSH stderr",
    )
    return result.stdout.decode()


def display(output):
    # Transport the exact config for checksum/race checks, not as user output.
    print(
        "\n".join(
            line for line in output.splitlines() if not line.startswith("grub_base64=")
        )
    )


def inspect():
    output = remote("status")
    fields = dict(line.split("=", 1) for line in output.splitlines() if "=" in line)
    for key in ("current", "profile", "grub_sha256", "grub_base64"):
        require(key in fields, "missing status field: " + key)
    require(re.fullmatch(SYSTEM, fields["current"]), "unexpected booted system")
    text = base64.b64decode(fields["grub_base64"], validate=True).decode()
    require(
        hashlib.sha256(text.encode()).hexdigest() == fields["grub_sha256"],
        "GRUB status checksum mismatch",
    )
    return output, fields, text


def archive_payload(firmware, grub):
    """Only regular, generated payloads, never device or symlink archive entries."""
    files = {"grub.cfg": grub.encode()}
    _, entries = parse_grub((firmware / "boot/grub/grub.cfg").read_text())
    for path in entries[0]["paths"]:
        files["payload/" + Path(path).name] = (firmware / path[1:]).read_bytes()
    files["BOOTAA64.EFI"] = (firmware / "EFI/BOOT/BOOTAA64.EFI").read_bytes()
    files["dejavu-mono.pf2"] = (firmware / "boot/grub/dejavu-mono.pf2").read_bytes()
    sums = "".join(
        hashlib.sha256(data).hexdigest() + "  " + name + "\n"
        for name, data in files.items()
    )
    files["SHA256SUMS"] = sums.encode()
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode="w") as archive:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o600
            archive.addfile(info, io.BytesIO(data))
    # Conservative: full payload size even if files already exist, plus room
    # for two configs, FAT rounding, temporary files and directory metadata.
    required = sum(len(data) for data in files.values()) + 2 * 64 * 1024 + 1024 * 1024
    return stream.getvalue(), required


def build(attribute, out_link):
    return Path(
        subprocess.check_output(
            [
                "nix",
                "build",
                "--out-link",
                str(out_link),
                "--print-out-paths",
                ".#" + attribute,
            ],
            cwd=ROOT,
            text=True,
        ).strip()
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_mutually_exclusive_group()
    actions.add_argument(
        "--status",
        action="store_true",
        help="read mount identity, booted/profile systems and GRUB selection",
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
            args.expected_system and re.fullmatch(SYSTEM, args.expected_system),
            "writes require --expected-system STORE_PATH",
        )
    output, before, previous = inspect()
    if args.status:
        display(output)
        return
    require(
        before["current"] == args.expected_system,
        "booted system does not match --expected-system",
    )
    if args.rollback:
        display(remote("rollback", before["current"], before["grub_sha256"]))
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
    with tempfile.TemporaryDirectory(prefix="rpminiv2-update-") as temp:
        # These links keep both results alive until staging has completed.
        system = build(
            "nixosConfigurations.rpminiv2.config.system.build.toplevel",
            Path(temp) / "system",
        )
        firmware = build(
            "nixosConfigurations.rpminiv2.config.system.build.rpminiv2Firmware",
            Path(temp) / "firmware",
        )
        new = (firmware / "boot/grub/grub.cfg").read_text()
        require(
            parse_grub(new)[1][0]["system"] == str(system), "firmware/system mismatch"
        )
        grub = updated_grub(new, previous, before["current"])
        archive, required = archive_payload(firmware, grub)
        # A fresh read catches another worker changing boot state during builds.
        _, now, _ = inspect()
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
            ["nix", "copy", "--to", "ssh-ng://" + TARGET, str(system)],
            check=True,
            env={**os.environ, "NIX_SSHOPTS": shlex.join(SSH_OPTIONS)},
        )
        display(
            remote(
                "update",
                before["current"],
                before["grub_sha256"],
                str(system),
                str(required),
                archive=archive,
            )
        )
        print("New system:", system)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit("Mini V2 update failed: " + str(error)) from error
