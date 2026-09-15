#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 util-linux e2fsprogs mtools dtc
"""Read-only acceptance for an uncompressed Retroid Pocket Mini V2 SD image.

Layout follows the Odin sd-image.nix systemd-boot producer, the NixOS SD
installer's 8 MiB offset, and Boot Loader Specification Type #1 entries.
Checks stored layout and references, not executable validity or hardware boot.
No mounts, device access, alternate DTBs, or filesystem fallback searches.
"""

import argparse
from contextlib import contextmanager
import os
from pathlib import Path
import posixpath
import re
import selectors
import stat
import struct
import subprocess
import sys
import tempfile
import time
import uuid
import zlib

SECTOR = 512
CHUNK = 1024 * 1024
CONFIG_LIMIT = 64 * 1024
OUTPUT_LIMIT = 1024 * 1024
ESP_GUID = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b"
ROOT_GUID = "b921b045-1df0-41c3-af44-4c6f280d3fae"
ENTRY = "nixos-generation-1.conf"
STORE_NAME = r"[0-9abcdfghijklmnpqrsvwxyz]{32}-[A-Za-z0-9+._?=-]+"


def require(condition, message):
    if not condition:
        raise ValueError(message)


@contextmanager
def regular_file(path):
    # As in the RG DS verifier, pin the inode before opening it for I/O.
    # O_PATH does not open a FIFO or device; O_NOFOLLOW rejects final symlinks.
    fd = os.open(path, os.O_PATH | os.O_NOFOLLOW)
    try:
        require(
            stat.S_ISREG(os.fstat(fd).st_mode),
            "expected a regular file, not a symlink or device",
        )
        with open(f"/proc/self/fd/{fd}", "rb") as stream:
            yield stream
    finally:
        os.close(fd)


def command(*args, fd=None):
    # Bound both pipes together and drain concurrently (RG DS algorithm).
    with subprocess.Popen(
        args,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        pass_fds=() if fd is None else (fd,),
        env={**os.environ, "LC_ALL": "C", "MTOOLSRC": "/dev/null"},
    ) as process:
        output = {process.stdout: bytearray(), process.stderr: bytearray()}
        deadline = time.monotonic() + 60
        total = 0
        try:
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


def read_at(image, offset, size):
    image.seek(offset)
    data = image.read(size)
    require(len(data) == size, "truncated image")
    return data


def partitions(image):
    size = os.fstat(image.fileno()).st_size
    require(size >= 8 * CHUNK and size % SECTOR == 0, "invalid image size")
    last = size // SECTOR - 1
    mbr = read_at(image, 0, SECTOR)
    require(
        mbr[510:] == b"\x55\xaa" and mbr[450] == 0xEE and mbr[462:510] == bytes(48),
        "expected a protective GPT MBR",
    )

    def header(lba, backup, table_lba):
        raw = read_at(image, lba * SECTOR, SECTOR)
        fields = struct.unpack("<8sIIIIQQQQ16sQIII", raw[:92])
        (
            signature,
            revision,
            length,
            checksum,
            reserved,
            current,
            alternate,
            first,
            end,
            disk_guid,
            entries_lba,
            count,
            width,
            table_crc,
        ) = fields
        require(
            signature == b"EFI PART"
            and revision == 0x10000
            and length == 92
            and reserved == 0,
            "invalid GPT header signature or format",
        )
        check = bytearray(raw[:length])
        check[16:20] = bytes(4)
        require(zlib.crc32(check) == checksum, "GPT header checksum mismatch")
        # sgdisk emits 128 entries of 128 bytes, with primary and backup arrays.
        require(
            current == lba
            and alternate == backup
            and entries_lba == table_lba
            and count == 128
            and width == 128
            and first == 34
            and end == last - 33,
            "GPT geometry differs from the declared image layout",
        )
        table = read_at(image, entries_lba * SECTOR, count * width)
        require(zlib.crc32(table) == table_crc, "GPT partition checksum mismatch")
        return disk_guid, table

    primary_guid, table = header(1, last, 2)
    backup_guid, backup_table = header(last, 1, last - 32)
    require(
        primary_guid == backup_guid and table == backup_table, "GPT copies disagree"
    )
    require(
        table[256:] == bytes(len(table) - 256), "expected exactly two GPT partitions"
    )
    parts = []
    for index, expected in enumerate((ESP_GUID, ROOT_GUID)):
        entry = table[index * 128 : (index + 1) * 128]
        require(
            str(uuid.UUID(bytes_le=entry[:16])) == expected,
            f"wrong partition {index + 1} type GUID",
        )
        start, end = struct.unpack_from("<QQ", entry, 32)
        require(34 <= start <= end <= last - 33, "partition bounds exceed the image")
        parts.append((start * SECTOR, (end - start + 1) * SECTOR))
    require(parts[0][0] == 8 * CHUNK, "ESP must start at 8 MiB")
    require(parts[1][0] == sum(parts[0]), "partition bounds must be contiguous")
    return parts


def filesystem(image, part, fs_type, label):
    offset, size = part
    fd = image.fileno()
    output = command(
        "blkid",
        "-p",
        "-o",
        "export",
        "-O",
        str(offset),
        "-S",
        str(size),
        f"/proc/self/fd/{fd}",
        fd=fd,
    )
    fields = dict(line.split("=", 1) for line in output.splitlines() if "=" in line)
    require(
        fields.get("TYPE") == fs_type and fields.get("LABEL") == label,
        f"expected {fs_type} filesystem label {label}",
    )
    if fs_type == "vfat":
        boot = read_at(image, offset, SECTOR)
        sector_size = struct.unpack_from("<H", boot, 11)[0]
        sectors16 = struct.unpack_from("<H", boot, 19)[0]
        sectors32 = struct.unpack_from("<I", boot, 32)[0]
        sectors = sectors16 or sectors32
        reserved = struct.unpack_from("<H", boot, 14)[0]
        fat_sectors = (
            struct.unpack_from("<H", boot, 22)[0]
            or struct.unpack_from("<I", boot, 36)[0]
        )
        root_entries = struct.unpack_from("<H", boot, 17)[0]
        metadata = (
            reserved
            + boot[16] * fat_sectors
            + (root_entries * 32 + SECTOR - 1) // SECTOR
        )
        require(
            reserved > 0
            and boot[16] in (1, 2)
            and fat_sectors > 0
            and metadata < sectors,
            "invalid FAT metadata geometry",
        )
        require(
            boot[510:] == b"\x55\xaa"
            and sector_size == SECTOR
            and boot[13] in (1, 2, 4, 8, 16, 32, 64, 128)
            and not (sectors16 and sectors32)
            and 0 < sectors * sector_size <= size,
            "invalid FAT signature or geometry",
        )


def extract_partition(image, part, target):
    offset, remaining = part
    image.seek(offset)
    with target.open("wb") as output:
        output.truncate(remaining)
        while remaining:
            data = image.read(min(CHUNK, remaining))
            require(data, "truncated partition")
            if data.count(0) == len(data):
                output.seek(len(data), os.SEEK_CUR)
            else:
                output.write(data)
            remaining -= len(data)


def ext4_geometry(root, partition_bytes):
    info = command("dumpe2fs", "-h", str(root))
    count = re.search(r"^Block count:\s+(\d+)\s*$", info, re.MULTILINE)
    size = re.search(r"^Block size:\s+(\d+)\s*$", info, re.MULTILINE)
    require(
        count and size and 0 < int(count[1]) * int(size[1]) <= partition_bytes,
        "ext4 geometry exceeds partition bounds or is invalid",
    )


def config(fat, path, allowed):
    text = command("mtype", "-i", str(fat), "::" + path)
    require(len(text.encode()) <= CONFIG_LIMIT, "boot config exceeds size limit")
    fields = {}
    for line in text.splitlines():
        words = line.strip().split(None, 1)
        if not words or words[0].startswith("#"):
            continue
        key = words[0]
        require(key in allowed, f"unexpected boot directive: {key}")
        require(
            key not in fields and len(words) == 2 and words[1].strip(),
            f"missing or duplicate boot directive: {key}",
        )
        fields[key] = words[1].strip()
    return fields


def fat_file(fat, path, target, partition_bytes):
    command("mcopy", "-i", str(fat), "::" + path, str(target))
    require(
        target.is_file() and 0 < target.stat().st_size <= partition_bytes,
        f"missing or empty boot file: {path}",
    )


def safe_store_path(path):
    # debugfs has its own command parser; exclude quotes, escapes, whitespace,
    # and traversal before passing any image-provided path to it.
    require(
        re.fullmatch(r"/nix/store/[A-Za-z0-9+._=/\-]+", path)
        and all(part not in ("", ".", "..") for part in path.split("/")[1:]),
        f"unsafe init path: {path!r}",
    )
    return path


def init_file(root, path):
    seen = set()
    for _ in range(16):
        safe_store_path(path)
        require(path not in seen, "init symlink loop")
        seen.add(path)
        info = command("debugfs", "-R", f'stat "{path}"', str(root))
        size = re.search(r"\bSize:\s+(\d+)", info)
        require(size and int(size[1]) > 0, f"missing or empty init: {path}")
        if re.search(r"\bType:\s+regular\b", info):
            return
        require(
            re.search(r"\bType:\s+symlink\b", info) and int(size[1]) <= 4096,
            "init must be a regular file or a bounded symlink",
        )
        fast = re.search(r'^Fast link dest: "(.*)"$', info, re.MULTILINE)
        target = (
            fast[1] if fast else command("debugfs", "-R", f'cat "{path}"', str(root))
        )
        require(len(target.encode()) == int(size[1]), "malformed init symlink")
        # Absolute targets are rooted in the extracted ext4, never on the host.
        path = (
            target
            if target.startswith("/")
            else posixpath.join(posixpath.dirname(path), target)
        )
    raise ValueError("init symlink chain exceeds limit")


def boot_files(fat, root, directory, fat_bytes):
    loader = config(fat, "/loader/loader.conf", {"default", "timeout", "console-mode"})
    require(
        loader.get("default") == ENTRY,
        "loader default must select nixos-generation-1.conf",
    )
    entry = config(
        fat,
        "/loader/entries/" + ENTRY,
        {"title", "version", "linux", "initrd", "devicetree", "options"},
    )
    fat_file(fat, "/EFI/BOOT/BOOTAA64.EFI", directory / "boot.efi", fat_bytes)
    for key in ("linux", "initrd", "devicetree"):
        path = entry.get(key, "")
        # The producer flattens store basenames below /EFI/nixos. Exclude FAT
        # wildcard syntax even though '?' is legal in a Nix store name.
        require(
            re.fullmatch(r"/EFI/nixos/" + STORE_NAME, path) and "?" not in path,
            f"missing or unsafe {key} reference",
        )
        fat_file(fat, path, directory / key, fat_bytes)
    require(
        len({entry[key] for key in ("linux", "initrd", "devicetree")}) == 3,
        "boot references must be distinct",
    )
    dtb = str(directory / "devicetree")
    require(
        command("fdtget", "-t", "s", dtb, "/", "model").strip()
        == "Retroid Pocket Mini V2",
        "wrong DTB model",
    )
    # Text output joins DT strings with spaces, losing their NUL boundaries.
    compatible = bytes(
        int(value, 16)
        for value in command("fdtget", "-t", "bx", dtb, "/", "compatible").split()
    )
    require(
        compatible == b"retroidpocket,rpminiv2\0qcom,sm8250\0",
        "wrong DTB compatible",
    )
    options = entry.get("options", "").split()
    init = [
        option.removeprefix("init=") for option in options if option.startswith("init=")
    ]
    require(
        len(init) == 1
        and re.fullmatch(
            r"/nix/store/[0-9abcdfghijklmnpqrsvwxyz]{32}-nixos-system-rpminiv2-[A-Za-z0-9+._=-]+/init",
            init[0],
        ),
        "options must contain one rpminiv2 system init reference",
    )
    # NixOS activation/top-level.nix copies bootStage2 (or systemd) to /init;
    # do not require an ELF header or a root= parameter supplied by the initrd.
    init_file(root, init[0])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path, help="uncompressed SD image regular file")
    args = parser.parse_args()
    try:
        with regular_file(args.image) as image:
            fat_part, root_part = partitions(image)
            filesystem(image, fat_part, "vfat", "RPMINIV2")
            filesystem(image, root_part, "ext4", "NIXOS_RPMINIV2")
            with tempfile.TemporaryDirectory(prefix="rpminiv2-verify-") as temp:
                directory = Path(temp)
                fat, root = directory / "esp.fat", directory / "root.ext4"
                extract_partition(image, fat_part, fat)
                extract_partition(image, root_part, root)
                ext4_geometry(root, root_part[1])
                boot_files(fat, root, directory, fat_part[1])
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"Retroid Pocket Mini V2 image rejected: {error}", file=sys.stderr)
        return 1
    print(f"Retroid Pocket Mini V2 image verified: {args.image}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
