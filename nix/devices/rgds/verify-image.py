#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 util-linux e2fsprogs
"""Read-only RG DS built-image acceptance; no mounts or device access.

Layout comes from ./sd-image.nix and the pinned NixOS installer/sd-card/
sd-image.nix. Boot paths follow generic-extlinux-compatible's generated config.
This checks stored files, not whether the hardware can execute them.
"""

import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import posixpath
import re
import selectors
import stat
import subprocess
import sys
import tempfile
import time


SECTOR = 512
FIRMWARE_START = 16 * 1024 * 1024
UBOOT_START = 64 * SECTOR
CONFIG = "/boot/extlinux/extlinux.conf"
DTB = "rockchip/rk3568-anbernic-rg-ds.dtb"
CHUNK = 1024 * 1024
CONFIG_LIMIT = 64 * 1024
OUTPUT_LIMIT = 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


@contextmanager
def regular_file(path):
    # O_PATH obtains an inode reference without opening a device for I/O, even
    # if the caller's path changes concurrently. Only reopen a checked regular
    # inode, through its descriptor rather than the caller's replaceable path.
    fd = os.open(path, os.O_PATH | os.O_NOFOLLOW)
    try:
        require(
            stat.S_ISREG(os.fstat(fd).st_mode),
            f"{path}: expected a regular file, not a symlink or device",
        )
        with open(f"/proc/self/fd/{fd}", "rb") as stream:
            yield stream
    finally:
        os.close(fd)


def command(*args, fd=None):
    with subprocess.Popen(
        args,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        pass_fds=() if fd is None else (fd,),
        env={**os.environ, "LC_ALL": "C"},
    ) as process:
        output = {process.stdout: bytearray(), process.stderr: bytearray()}
        deadline = time.monotonic() + 60
        total = 0
        try:
            # Drain both pipes concurrently so stderr cannot deadlock stdout.
            # Bound their combined size before retaining each new chunk.
            with selectors.DefaultSelector() as selector:
                for pipe in output:
                    selector.register(pipe, selectors.EVENT_READ)
                while selector.get_map():
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise subprocess.TimeoutExpired(args, 60)
                    for key, _ in selector.select(remaining):
                        data = os.read(key.fd, 64 * 1024)
                        if not data:
                            selector.unregister(key.fileobj)
                            continue
                        total += len(data)
                        require(
                            total <= OUTPUT_LIMIT, f"{args[0]} exceeded output limit"
                        )
                        output[key.fileobj].extend(data)
            process.wait(timeout=max(0, deadline - time.monotonic()))
        finally:
            if process.poll() is None:
                process.kill()
            process.wait()
        require(
            process.returncode == 0,
            f"{args[0]} failed: {output[process.stderr].decode(errors='replace').strip()}",
        )
        return output[process.stdout].decode("utf-8")


def partitions(image):
    fd = image.fileno()
    table = json.loads(command("sfdisk", "--json", f"/proc/self/fd/{fd}", fd=fd))[
        "partitiontable"
    ]
    require(
        table["label"] == "dos"
        and table["unit"] == "sectors"
        and table["sectorsize"] == SECTOR,
        "expected an MBR table with 512-byte sectors",
    )
    parts = table["partitions"]
    require(len(parts) == 2, "expected exactly two partitions")
    firmware, root = parts
    require(
        firmware["start"] * SECTOR == FIRMWARE_START and firmware["type"] == "b",
        "partition 1 must be FAT at the 16 MiB offset",
    )
    require(not firmware.get("bootable", False), "partition 1 must not be bootable")
    require(
        root["type"] == "83" and root.get("bootable", False),
        "partition 2 must be bootable Linux",
    )
    require(
        root["start"] == firmware["start"] + firmware["size"],
        "partition bounds must be contiguous",
    )
    for part in parts:
        require(
            part["size"] > 0
            and (part["start"] + part["size"]) * SECTOR <= os.fstat(fd).st_size,
            "partition bounds exceed the image",
        )
    return parts


def filesystem(image, part, fs_type, label):
    fd = image.fileno()
    # Probe only inside this partition, never the host's device inventory.
    output = command(
        "blkid",
        "-p",
        "-o",
        "export",
        "-O",
        str(part["start"] * SECTOR),
        "-S",
        str(part["size"] * SECTOR),
        f"/proc/self/fd/{fd}",
        fd=fd,
    )
    fields = dict(line.split("=", 1) for line in output.splitlines() if "=" in line)
    require(
        fields.get("TYPE") == fs_type and fields.get("LABEL") == label,
        f"expected {fs_type} filesystem label {label}",
    )


def compare_uboot(image, uboot):
    size = os.fstat(uboot.fileno()).st_size
    require(
        0 < size <= FIRMWARE_START - UBOOT_START,
        "U-Boot must be nonempty and fit before partition 1",
    )
    image.seek(UBOOT_START)
    remaining = size
    while remaining:
        data = uboot.read(min(CHUNK, remaining))
        require(
            data and image.read(len(data)) == data,
            "embedded U-Boot differs from u-boot-rockchip.bin",
        )
        remaining -= len(data)


def extract_root(image, part, target):
    image.seek(part["start"] * SECTOR)
    remaining = part["size"] * SECTOR
    with target.open("wb") as root:
        root.truncate(remaining)
        while remaining:
            data = image.read(min(CHUNK, remaining))
            require(data, "truncated root partition")
            # Preserve sparse space; production roots can be much larger than
            # these small tests. Memory stays bounded to one chunk.
            if data.count(0) == len(data):
                root.seek(len(data), os.SEEK_CUR)
            else:
                root.write(data)
            remaining -= len(data)


def ext4_geometry(root, partition_bytes):
    info = command("dumpe2fs", "-h", str(root))
    count = re.search(r"^Block count:\s+(\d+)\s*$", info, re.MULTILINE)
    size = re.search(r"^Block size:\s+(\d+)\s*$", info, re.MULTILINE)
    require(
        count and size and 0 < int(count[1]) * int(size[1]) <= partition_bytes,
        "ext4 geometry exceeds partition bounds or is invalid",
    )


def debugfs(root, operation, path):
    # debugfs parses its own command string even without a shell. Never permit
    # a config path to add another argument or command.
    require(not any(c in path for c in '\\"\n\r\x00'), f"unsafe boot path: {path!r}")
    return command("debugfs", "-R", f'{operation} "{path}"', str(root))


def nonempty_file(root, path):
    # debugfs can exit zero on a missing file; inspect stat rather than its exit.
    info = debugfs(root, "stat", path)
    size = re.search(r"\bSize:\s+(\d+)", info)
    require(
        re.search(r"\bType:\s+regular\b", info) and size and int(size[1]) > 0,
        f"missing or empty regular boot file: {path}",
    )
    return int(size[1])


def boot_files(root):
    require(
        nonempty_file(root, CONFIG) <= CONFIG_LIMIT,
        f"{CONFIG} exceeds {CONFIG_LIMIT}-byte config limit",
    )
    entries = {}
    default = None
    current = None
    for line in debugfs(root, "cat", CONFIG).splitlines():
        words = line.strip().split(None, 1)
        if not words or words[0].startswith("#"):
            continue
        key = words[0].upper()
        value = words[1].strip() if len(words) == 2 else ""
        if key == "DEFAULT":
            require(default is None and value, "expected one nonempty DEFAULT")
            default = value
        elif key == "LABEL":
            require(
                value and value not in entries, "expected unique nonempty LABEL entries"
            )
            current = entries[value] = {}
        elif key in ("LINUX", "INITRD", "FDT") and current is not None:
            require(key not in current, f"duplicate {key} in boot entry")
            current[key] = value
    require(default in entries, "DEFAULT does not select a boot entry")
    entry = entries[default]
    for key in ("LINUX", "INITRD", "FDT"):
        require(entry.get(key), f"DEFAULT entry needs nonempty {key}")
        # NixOS emits ../nixos/... relative to /boot/extlinux; absolute paths
        # are relative to the root filesystem, not this verifier's host.
        path = posixpath.normpath(posixpath.join(posixpath.dirname(CONFIG), entry[key]))
        if key == "FDT":
            require(path.endswith("/" + DTB), f"FDT must reference {DTB}")
        nonempty_file(root, path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path, help="uncompressed SD image regular file")
    parser.add_argument(
        "uboot", type=Path, help="matching u-boot-rockchip.bin regular file"
    )
    args = parser.parse_args()
    try:
        with regular_file(args.image) as image, regular_file(args.uboot) as uboot:
            firmware, root = partitions(image)
            compare_uboot(image, uboot)
            filesystem(image, firmware, "vfat", "NIXOS_BOOT")
            filesystem(image, root, "ext4", "NIXOS_RGDS")
            with tempfile.TemporaryDirectory(prefix="rgds-verify-") as directory:
                extracted = Path(directory) / "root.ext4"
                extract_root(image, root, extracted)
                ext4_geometry(extracted, root["size"] * SECTOR)
                boot_files(extracted)
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        print(f"RG DS image rejected: {error}", file=sys.stderr)
        return 1
    print(f"RG DS image verified: {args.image}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
