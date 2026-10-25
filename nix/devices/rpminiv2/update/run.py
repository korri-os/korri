#!/usr/bin/env python3
"""Build, sign and stage an internal-UFS Mini V2 system from Zao. Never reboot."""

import hashlib
import io
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile

HERE = Path(__file__).resolve().parent
ROOT = Path(os.environ.get("KORRI_ROOT", HERE.parents[3]))
TARGET = "root@10.42.3.1"
SYSTEM = r"/nix/store/[0-9abcdfghijklmnpqrsvwxyz]{32}-nixos-system-rpminiv2-sd-card-[A-Za-z0-9+._=-]+"
PAYLOAD = r"/EFI/nixos/[0-9abcdfghijklmnpqrsvwxyz]{32}-[A-Za-z0-9+._=-]+"
DEVICE = "rpminiv2"
CONFIG_NAME = "grub"
# Both devices use the existing SSH/signing/profile transaction, not a new trust path.
sys.path.insert(0, str(HERE.parents[1] / "system-update"))
from runner import UpdateDevice, build, main, require  # noqa: E402


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


def prepare(system: Path, temp: Path, previous: str, current: str) -> tuple[bytes, int]:
    firmware = build(
        UPDATE,
        "nixosConfigurations.rpminiv2.config.system.build.rpminiv2Firmware",
        temp / "firmware",
    )
    new = (firmware / "boot/grub/grub.cfg").read_text()
    require(parse_grub(new)[1][0]["system"] == str(system), "firmware/system mismatch")
    return archive_payload(firmware, updated_grub(new, previous, current))


UPDATE = UpdateDevice(
    description=__doc__,
    HERE=HERE,
    ROOT=ROOT,
    DEVICE=DEVICE,
    TARGET=TARGET,
    SYSTEM=SYSTEM,
    CONFIG_NAME=CONFIG_NAME,
    prepare=prepare,
)


if __name__ == "__main__":
    try:
        main(UPDATE)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit("Mini V2 update failed: " + str(error)) from error
