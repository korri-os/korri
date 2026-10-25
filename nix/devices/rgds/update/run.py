#!/usr/bin/env python3
"""Build/sign on Zao and stage an RG DS eMMC update over SSH. Never reboot."""

import hashlib
import io
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tarfile
from typing import TypedDict

HERE = Path(__file__).resolve().parent
ROOT = Path(os.environ.get("KORRI_ROOT", HERE.parents[3]))
sys.path.insert(0, str(HERE.parents[1] / "system-update"))
from runner import SSH_OPTIONS, UpdateDevice, build, main, require  # noqa: E402

DEVICE = "rgds"
CONFIG_NAME = "extlinux"
TARGET = "root@10.42.1.1"
SYSTEM = r"/nix/store/[0-9abcdfghijklmnpqrsvwxyz]{32}-nixos-system-rgds-sd-card-[A-Za-z0-9+._=-]+"
# A specialisation's system carries its tag before "sd-card"; the product
# ships the development specialisation on every generation (product/boot.nix).
SPECIALISATION_SYSTEM = r"/nix/store/[0-9abcdfghijklmnpqrsvwxyz]{32}-nixos-system-rgds-[a-z0-9]+-sd-card-[A-Za-z0-9+._=-]+"
PAYLOAD = r"[0-9abcdfghijklmnpqrsvwxyz]{32}-[A-Za-z0-9+._=-]+"
BUILD_OPTIONS = [
    "--option",
    "extra-substituters",
    "https://korri-os.github.io/nix-cache/",
    "--option",
    "extra-trusted-public-keys",
    "korri-cache-ci-1:iH8gsPMtGrreeuXt2kt6M2ca+y4u/dO+T2km6AUuI8c=",
]
DTB = "rockchip/rk3568-anbernic-rg-ds.dtb"


class BootEntry(TypedDict):
    # Derived only from the native LABEL, APPEND init= and LINUX/INITRD/FDT
    # directives.
    label: str
    system: str
    paths: list[str]


def default_entries(entries: list[BootEntry]) -> list[BootEntry]:
    """The generator's default entry and each generation's default entry."""
    return [
        e
        for e in entries
        if e["label"] == "nixos-default"
        or re.fullmatch(r"nixos-\d+-default", e["label"])
    ]


def check_specialisations(entries: list[BootEntry], previous: str) -> None:
    """Non-default entries must be the previous system's own specialisations.

    The pinned generator renders one entry per specialisation of each generation
    (`addEntry $link "${generation}-${specialisation}"`), never for the new default.
    """
    for entry in entries:
        if entry in default_entries(entries):
            continue
        match = re.fullmatch(r"nixos-1-([a-z0-9]+)", entry["label"])
        if match is None:
            raise ValueError("unexpected boot entry label")
        target = Path(previous) / "specialisation" / match.group(1)
        require(
            target.is_symlink() and str(target.resolve()) == entry["system"],
            "boot entry is not a specialisation of the previous system",
        )


def parse_extlinux(text: str) -> tuple[dict[str, str], list[BootEntry]]:
    """Accept the production generator's dialect; never evaluate boot directives."""
    require(len(text.encode()) <= 64 * 1024, "extlinux config exceeds 64 KiB")
    header: dict[str, str] = {}
    entries: list[dict[str, str]] = []
    for line in text.splitlines():
        words = line.strip().split(None, 1)
        if not words or words[0].startswith("#"):
            continue
        require(len(words) == 2, "empty extlinux directive")
        key, value = words
        if key == "LABEL":
            require(re.fullmatch(r"nixos-[A-Za-z0-9-]+", value), "unsafe entry label")
            entries.append({"LABEL": value})
            continue
        fields = entries[-1] if entries else header
        # MENU is one native directive with two possible sub-directives.
        require(key not in fields, "duplicate extlinux directive")
        fields[key] = value
    require(set(header) == {"DEFAULT", "TIMEOUT", "MENU"}, "unexpected extlinux header")
    require(header["DEFAULT"] == "nixos-default", "unexpected extlinux default")
    require(
        header["TIMEOUT"].isdigit() and header["MENU"].startswith("TITLE "),
        "invalid menu header",
    )
    require(
        entries and entries[0]["LABEL"] == header["DEFAULT"], "default must be first"
    )
    require(len({e["LABEL"] for e in entries}) == len(entries), "duplicate entry label")
    result: list[BootEntry] = []
    for entry in entries:
        require(
            set(entry) == {"LABEL", "MENU", "LINUX", "INITRD", "APPEND", "FDT"},
            "unexpected boot directives",
        )
        require(entry["MENU"].startswith("LABEL "), "missing menu label")
        paths = [entry[key] for key in ("LINUX", "INITRD", "FDT")]
        for path in paths:
            require(
                re.fullmatch(
                    r"\.\./nixos/" + PAYLOAD + r"(?:/" + re.escape(DTB) + r")?", path
                ),
                "unsafe boot payload path",
            )
        require(entry["FDT"].endswith("/" + DTB), "wrong RG DS device tree")
        params = entry["APPEND"].split()
        init = [p[5:] for p in params if p.startswith("init=")]
        require(
            len(init) == 1 and init[0].endswith("/init"), "expected one system init"
        )
        system = init[0][:-5]
        require(
            re.fullmatch(SYSTEM, system) or re.fullmatch(SPECIALISATION_SYSTEM, system),
            "unsafe RG DS system path",
        )
        result.append({"label": entry["LABEL"], "system": system, "paths": paths})
    return header, result


def render(
    builder: Path,
    system: Path,
    previous: Path | str,
    dest: Path,
    *,
    timeout: int | None,
    dtb: str,
) -> None:
    """Run the pinned native generator with an isolated native Nix profile set.

    Upstream hard-codes the host profile directory. Redirect only that I/O edge;
    all rendering, timestamps, filenames, copies and pruning remain upstream's.
    """
    profiles = dest.parent / "profiles"
    profiles.mkdir()
    (profiles / "system-1-link").symlink_to(previous)
    source = builder.read_text()
    require(
        "/nix/var/nix/profiles" in source,
        "upstream generator profile interface changed",
    )
    script = dest.parent / "extlinux-conf-builder.sh"
    script.write_text(source.replace("/nix/var/nix/profiles", str(profiles)))
    script.chmod(0o700)
    subprocess.run(
        [
            str(script),
            "-g",
            "1",
            "-t",
            str(-1 if timeout is None else timeout),
            "-n",
            dtb,
            "-c",
            str(system),
            "-d",
            str(dest),
        ],
        check=True,
        env={**os.environ, "TZ": "UTC"},
    )


def archive_payload(tree: Path) -> tuple[bytes, int]:
    text = (tree / "extlinux/extlinux.conf").read_text()
    _, entries = parse_extlinux(text)
    defaults = default_entries(entries)
    require(
        len(defaults) == 2 and entries[: len(defaults)] == defaults,
        "expected new and previous default entries first",
    )
    require(
        all(re.fullmatch(SYSTEM, e["system"]) for e in defaults),
        "default entries must boot a default RG DS system",
    )
    files = {"extlinux.conf": text.encode()}
    for path in sorted((tree / "nixos").rglob("*")):
        require(not path.is_symlink(), "symlink in generated tree")
        if path.is_file():
            files["payload/" + path.relative_to(tree / "nixos").as_posix()] = (
                path.read_bytes()
            )
    for entry in entries:
        for payload_path in entry["paths"]:
            require(
                "payload/" + payload_path.removeprefix("../nixos/") in files,
                "missing generated payload",
            )
    files["SHA256SUMS"] = "".join(
        hashlib.sha256(data).hexdigest() + "  " + name + "\n"
        for name, data in files.items()
    ).encode()
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode="w") as archive:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size, info.mode = len(data), 0o600
            archive.addfile(info, io.BytesIO(data))
    return stream.getvalue(), sum(
        map(len, files.values())
    ) + 2 * 64 * 1024 + 1024 * 1024


def prepare(system: Path, temp: Path, previous: str, current: str) -> tuple[bytes, int]:
    require(
        parse_extlinux(previous)[1][0]["system"] == current,
        "extlinux default differs from booted system",
    )
    require(str(system) != current, "already running this system")
    # Retrieve the old prebuilt closure for the real generator; never realise or
    # evaluate a derivation on the handheld. Nix still requires trusted signatures.
    subprocess.run(
        ["nix", "copy", "--from", "ssh-ng://" + TARGET, current],
        check=True,
        env={**os.environ, "NIX_SSHOPTS": shlex.join(SSH_OPTIONS)},
    )
    # Root the local copy until rendering completes, even during concurrent GC.
    subprocess.run(
        [
            "nix-store",
            "--add-root",
            str(temp / "previous"),
            "--realise",
            current,
            "--option",
            "max-jobs",
            "0",
            "--option",
            "builders",
            "",
            "--option",
            "fallback",
            "false",
        ],
        check=True,
    )
    host = subprocess.check_output(
        ["nix", "eval", "--impure", "--raw", "--expr", "builtins.currentSystem"],
        text=True,
    ).strip()
    builder = build(
        UPDATE,
        "packages." + host + ".rgds-update-renderer",
        temp / "renderer",
    )
    tree = temp / "boot"
    # Native loader options have one producer: the existing RG DS module.
    timeout, dtb = json.loads(
        subprocess.check_output(
            [
                "nix",
                "eval",
                "--json",
                ".#nixosConfigurations.rgds.config",
                "--apply",
                "c: [ c.boot.loader.timeout c.hardware.deviceTree.name ]",
            ],
            cwd=ROOT,
            text=True,
        )
    )
    require(
        (timeout is None or type(timeout) is int) and dtb == DTB,
        "unexpected loader options",
    )
    render(builder, system, current, tree, timeout=timeout, dtb=dtb)
    new = (tree / "extlinux/extlinux.conf").read_text()
    header, entries = parse_extlinux(new)
    require(
        header == parse_extlinux(previous)[0], "extlinux handoff differs from image"
    )
    require(
        [e["system"] for e in default_entries(entries)] == [str(system), current],
        "boot tree/system mismatch",
    )
    check_specialisations(entries, current)
    return archive_payload(tree)


UPDATE = UpdateDevice(
    description=__doc__,
    HERE=HERE,
    ROOT=ROOT,
    DEVICE=DEVICE,
    TARGET=TARGET,
    SYSTEM=SYSTEM,
    CONFIG_NAME=CONFIG_NAME,
    prepare=prepare,
    BUILD_OPTIONS=tuple(BUILD_OPTIONS),
)


if __name__ == "__main__":
    try:
        main(UPDATE)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit("RG DS update failed: " + str(error)) from error
