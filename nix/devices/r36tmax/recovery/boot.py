"""R36T Max's proven hybrid boot contract, applied only to regular files.

System inputs mirror addEntry in nixpkgs' generic-extlinux-compatible builder:
kernel, initrd, dtbs, and APPEND init=<resolved system>/init <kernel-params>.
The filenames and ramdisk address come from the preserved stock-shell
hybrid-boot procedure, without runtime overrides.
"""

import argparse
from collections import deque
import gzip
import hashlib
from pathlib import Path
import re
import shutil
import stat
import struct
import subprocess
import tempfile
import zlib

ARCHIVE_SHA256 = "e3272fc4266363332b830612db1abe4e20bb6a17c58d3c9f32815997556c768e"
LOADER_SHA256 = "0c88ade1572385515331cb6ea1c1ba6368c6ef867ef51ad59a57cafbfcff6c7a"
SCRIPT_SHA256 = "c61a5760f5073a81651620bd223541d267e5b0f0a9ed510609f6631275307531"
LOADER_START = 64 * 512
FIRMWARE_START = 32768 * 512
FAT_BYTES = 128 * 1024 * 1024
KERNEL_LOAD_BYTES = 0x0C000000 - 0x09000000
BOARD_DTB = "rockchip/rk3326-aislpc-r36t-max.dtb"
DTB_NAMES = (
    "rk3326-odroid-go2.dtb",
    "rk3326-odroid-go2-v11.dtb",
    "rk3326-odroid-go3.dtb",
    "rk3326-gameconsole-r33s.dtb",
    "rk3326-gameforce-chi.dtb",
    "rk3326-anbernic-rg351v.dtb",
    "rk3326-anbernic-rg351m.dtb",
    "rk3326-powkiddy-rgb10.dtb",
    "rk3326-magicx-xu10.dtb",
)
RAMDISK_LINE = b'setenv ramdisk_addr_r "0x0c000000"\n'


def regular(path):
    if not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f"missing or empty regular file: {path}")
    return path


def digest(path):
    with regular(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def require_hash(path, expected):
    actual = digest(path)
    if actual != expected:
        raise ValueError(f"SHA256 mismatch for {path}: {actual}, expected {expected}")


def script_text(image):
    if len(image) < 72:
        raise ValueError("truncated legacy boot.scr")
    magic, hcrc, _, size, _, _, dcrc, _, _, kind, _, _ = struct.unpack(
        ">7I4B32s", image[:64]
    )
    header = image[:4] + bytes(4) + image[8:64]
    payload = image[64:]
    if magic != 0x27051956 or kind != 6:
        raise ValueError("not a U-Boot legacy script")
    if (
        zlib.crc32(header) != hcrc
        or len(payload) != size
        or zlib.crc32(payload) != dcrc
    ):
        raise ValueError("boot.scr size or CRC mismatch")
    length, terminator = struct.unpack(">II", payload[:8])
    if terminator != 0 or length > len(payload) - 8 or any(payload[8 + length :]):
        raise ValueError("invalid legacy script component lengths")
    text = payload[8 : 8 + length]
    if b"\0" in text:
        raise ValueError("NUL inside boot script")
    return text


def patch_script(text):
    if b"ramdisk_addr_r" in text:
        raise ValueError("source already contains ramdisk_addr_r")
    lines = text.splitlines(keepends=True)
    if sum(line.startswith(b"sysboot ") for line in lines) != 1:
        raise ValueError("expected exactly one sysboot line")
    return b"".join(
        RAMDISK_LINE + line if line.startswith(b"sysboot ") else line for line in lines
    )


def patched_script(path):
    text = script_text(regular(path).read_bytes())
    original = text.replace(RAMDISK_LINE, b"")
    if text.count(RAMDISK_LINE) != 1 or patch_script(original) != text:
        raise ValueError("missing or misplaced ramdisk_addr_r patch")


def partitions(image):
    with regular(image).open("rb") as source:
        mbr = source.read(512)
    if len(mbr) != 512 or mbr[510:] != b"\x55\xaa":
        raise ValueError("missing DOS partition table")
    result = []
    for index in range(4):
        _, _, kind, _, start, size = struct.unpack(
            "<B3sB3sII", mbr[446 + index * 16 : 462 + index * 16]
        )
        result.append((kind, start * 512, size * 512))
    return result


def copy_region(image, output, start, size):
    with regular(image).open("rb") as source, output.open("wb") as target:
        source.seek(start)
        while size:
            chunk = source.read(min(size, 1024 * 1024))
            if not chunk:
                raise ValueError(f"truncated image: {image}")
            target.write(chunk)
            size -= len(chunk)


def extract(archive, destination):
    # Hash the complete compressed release, not an unverified ranged download.
    require_hash(archive, ARCHIVE_SHA256)
    destination.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory() as work:
        image = Path(work) / "rocknix.img"
        with gzip.open(archive, "rb") as source, image.open("wb") as target:
            shutil.copyfileobj(source, target, 1024 * 1024)
        kind, offset, size = partitions(image)[0]
        if (
            kind not in (0x0B, 0x0C)
            or offset != FIRMWARE_START
            or offset + size > image.stat().st_size
        ):
            raise ValueError("unexpected ROCKNIX FAT partition")
        loader = destination / "rocknix-loader-full.bin"
        copy_region(image, loader, LOADER_START, FIRMWARE_START - LOADER_START)
        require_hash(loader, LOADER_SHA256)
        original = destination / "boot.scr.original"
        subprocess.run(
            ["mcopy", "-i", f"{image}@@{offset}", "::/boot.scr", str(original)],
            check=True,
        )
        require_hash(original, SCRIPT_SHA256)
        text = script_text(original.read_bytes())
        # A changed detector must not silently acquire incomplete alias coverage.
        detected = set(re.findall(rb"rk3326-[a-zA-Z0-9-]+\.dtb", text))
        if detected != {name.encode() for name in DTB_NAMES}:
            raise ValueError(f"unexpected boot.scr detection names: {detected}")
        script = destination / "boot.txt"
        script.write_bytes(patch_script(text))
        subprocess.run(
            [
                "mkimage",
                "-A",
                "ppc",
                "-O",
                "linux",
                "-T",
                "script",
                "-C",
                "gzip",
                "-n",
                "",
                "-d",
                str(script),
                str(destination / "boot.scr"),
            ],
            check=True,
        )
        patched_script(destination / "boot.scr")


def contract(system, loader):
    system = system.resolve(strict=True)
    inputs = {
        "Image": regular(system / "kernel"),
        "initrd": regular(system / "initrd"),
        "boot.scr": regular(loader / "boot.scr"),
    }
    regular(system / "init")
    dtb = regular(system / "dtbs" / BOARD_DTB)
    inputs.update({name: dtb for name in DTB_NAMES})
    # Bash command substitution in the upstream builder strips trailing LF only.
    params = regular(system / "kernel-params").read_text().rstrip("\n")
    if any(char in params for char in "\r\n\0"):
        raise ValueError("kernel-params must be a single line")
    if any(param.startswith("init=") for param in params.split()):
        raise ValueError("kernel-params must not replace the system init= path")
    patched_script(inputs["boot.scr"])
    conf = (
        "DEFAULT korri\nLABEL korri\n"
        "  LINUX /Image\n  INITRD /initrd\n  FDTDIR /\n"
        f"  APPEND init={system}/init {params}\n"
    ).encode()
    # hybrid-boot loads Image at 0x09000000 and initrd at 0x0c000000.
    # The FAT budget alone cannot rule out overlapping those RAM load extents.
    if inputs["Image"].stat().st_size > KERNEL_LOAD_BYTES:
        raise ValueError("kernel Image exceeds 48 MiB RAM load gap before initrd")
    # Conservative cluster/directory allowance; real FAT copying is also checked.
    needed = (
        sum(path.stat().st_size for path in inputs.values()) + len(conf) + 1024 * 1024
    )
    if needed > FAT_BYTES:
        raise ValueError(f"boot files need {needed} bytes: exceed 128 MiB FAT budget")
    return inputs, conf


def populate(system, loader, destination):
    inputs, conf = contract(system, loader)
    if destination.exists() and (
        not destination.is_dir() or any(destination.iterdir())
    ):
        raise ValueError(f"destination must be empty: {destination}")
    destination.mkdir(parents=True, exist_ok=True)
    for name, source in inputs.items():
        shutil.copyfile(source, destination / name)
    (destination / "extlinux").mkdir()
    (destination / "extlinux/extlinux.conf").write_bytes(conf)
    verify_directory(system, loader, destination)


def verify_directory(system, loader, directory):
    inputs, conf = contract(system, loader)
    for name, source in inputs.items():
        if digest(directory / name) != digest(source):
            raise ValueError(f"boot artifact differs from system input: {name}")
    if regular(directory / "extlinux/extlinux.conf").read_bytes() != conf:
        raise ValueError("extlinux configuration differs from the system contract")
    actual = {
        str(path.relative_to(directory))
        for path in directory.rglob("*")
        if not path.is_dir()
    }
    if actual != {*inputs, "extlinux/extlinux.conf"}:
        raise ValueError(
            f"unexpected boot files: {actual - {*inputs, 'extlinux/extlinux.conf'}}"
        )


def verify_fat(system, loader, image):
    if regular(image).stat().st_size != FAT_BYTES:
        raise ValueError("expected a 128 MiB FAT image")
    label = subprocess.run(
        ["mlabel", "-i", str(image), "-s", "::"],
        check=True,
        capture_output=True,
    ).stdout
    if label.removesuffix(b"\n").rstrip(b" ") != b" Volume label is NIXOS_BOOT":
        raise ValueError("FAT filesystem label must be NIXOS_BOOT")
    with tempfile.TemporaryDirectory() as work:
        subprocess.run(["mcopy", "-i", str(image), "-s", "::/*", work], check=True)
        verify_directory(system, loader, Path(work))


def verify_image(system, loader, image):
    entries = partitions(image)
    kind, start, size = entries[0]
    root_kind, root_start, root_size = entries[1]
    if kind not in (0x0B, 0x0C) or start != FIRMWARE_START or size != FAT_BYTES:
        raise ValueError("unexpected recovery FAT partition layout")
    if (
        root_kind != 0x83
        or root_start < start + size
        or root_start + root_size > image.stat().st_size
    ):
        raise ValueError("unexpected recovery root partition layout")
    if any(entry != (0, 0, 0) for entry in entries[2:]):
        raise ValueError("unexpected extra partitions")
    with tempfile.TemporaryDirectory() as work:
        work = Path(work)
        raw = work / "loader.bin"
        copy_region(image, raw, LOADER_START, FIRMWARE_START - LOADER_START)
        require_hash(raw, LOADER_SHA256)
        fat = work / "fat.img"
        copy_region(image, fat, start, size)
        verify_fat(system, loader, fat)
        root = work / "root.img"
        copy_region(image, root, root_start, root_size)
        verify_root(system, root)


class ModuleImage:
    """Compare only the supplied module subtree and its linked image targets."""

    def __init__(self, image, work):
        self.image = image
        self.content = work / "module-content"
        self.verified = set()
        self.stats = {}

    def command(self, operation, *paths):
        # debugfs has its own command parser, not a shell. Nix module paths do
        # not need its quoting/escape syntax; reject ambiguous names outright.
        arguments = []
        for path in paths:
            if any(char in str(path) for char in '\\"\r\n\0'):
                raise ValueError(f"unsupported module path: {path}")
            arguments.append(f'"{path}"')
        return subprocess.run(
            ["debugfs", "-R", " ".join([operation, *arguments]), str(self.image)],
            check=True,
            capture_output=True,
            text=True,
        ).stdout

    def inode(self, path):
        if path not in self.stats:
            output = self.command("stat", path)
            kind = re.search(r"^Inode: \d+\s+Type: (\w+)", output)
            if kind is None:
                raise ValueError(f"module path absent from root: {path}")
            self.stats[path] = kind[1], output
        return self.stats[path]

    def dump(self, path):
        self.content.unlink(missing_ok=True)
        self.command("dump", path, self.content)
        # Empty dependency indexes are valid; a failed debugfs dump is not.
        if not self.content.is_file():
            raise ValueError(f"cannot read root module file: {path}")
        return self.content

    def resolve(self, path):
        pending = deque(path.parts[1:])
        current = Path("/")
        links = 0
        while pending:
            part = pending.popleft()
            if part == "..":
                current = current.parent
                continue
            candidate = current / part
            kind, output = self.inode(candidate)
            try:
                mode = candidate.lstat().st_mode
            except OSError as error:
                raise ValueError(
                    f"invalid supplied module path: {candidate}"
                ) from error
            expected_kind = {
                stat.S_IFLNK: "symlink",
                stat.S_IFDIR: "directory",
                stat.S_IFREG: "regular",
            }.get(stat.S_IFMT(mode), "unsupported")
            if kind != expected_kind or expected_kind == "unsupported":
                raise ValueError(
                    f"root module path must be {expected_kind}: {candidate}"
                )
            if kind == "symlink":
                expected = str(candidate.readlink())
                inline = re.search(r'^Fast link dest: "(.*)"$', output, re.MULTILINE)
                actual = inline[1] if inline else self.dump(candidate).read_text()
                if actual != expected:
                    raise ValueError(f"root module symlink differs: {candidate}")
                links += 1
                if links > 40:
                    raise ValueError(f"module symlink loop: {path}")
                target = Path(actual)
                if target.is_absolute():
                    current = Path("/")
                    pending.extendleft(reversed(target.parts[1:]))
                else:
                    pending.extendleft(reversed(target.parts))
            else:
                if pending and kind != "directory":
                    raise ValueError(
                        f"root module path is not a directory: {candidate}"
                    )
                current = candidate
        return current

    def compare(self, path):
        # Never follow an extracted absolute symlink with host filesystem APIs.
        # Resolve each image component and compare link text before reading it.
        path = self.resolve(path)
        if path in self.verified:
            return
        self.verified.add(path)
        kind, _ = self.inode(path)
        if kind == "directory":
            expected = {child.name for child in path.iterdir()}
            actual = set()
            for line in self.command("ls -p", path).splitlines():
                if not line:
                    continue
                entry = re.fullmatch(r"/(\d+)/\d+/\d+/\d+/([^/]*)/\d*/", line)
                if entry is None:
                    raise ValueError(f"invalid root module directory listing: {path}")
                if entry[1] != "0" and entry[2] not in (".", ".."):
                    actual.add(entry[2])
            if actual != expected:
                raise ValueError(
                    f"root module directory differs: {path}; "
                    f"missing={sorted(expected - actual)}, extra={sorted(actual - expected)}"
                )
            for name in sorted(expected):
                self.compare(path / name)
        else:
            with path.open("rb") as source, self.dump(path).open("rb") as image:
                if (
                    hashlib.file_digest(source, "sha256").digest()
                    != hashlib.file_digest(image, "sha256").digest()
                ):
                    raise ValueError(f"root module file differs: {path}")


def verify_root(system, root):
    regular(root)
    label = subprocess.run(
        ["e2label", str(root)], check=True, capture_output=True
    ).stdout
    # e2label appends one LF. Do not strip spaces or normalize CR in the label.
    if label != b"NIXOS_R36TMAX\n":
        raise ValueError("ext4 filesystem label must be NIXOS_R36TMAX")
    with tempfile.TemporaryDirectory() as work:
        work = Path(work)
        # debugfs returns success even for some missing paths. Dump then compare,
        # rather than treating its process status as proof of file existence.
        system = system.resolve(strict=True)
        for name in ("init", "kernel-params"):
            target = work / name
            subprocess.run(
                ["debugfs", "-R", f"dump {system}/{name} {target}", str(root)],
                check=True,
            )
            if digest(target) != digest(system / name):
                raise ValueError(f"root system differs: {name}")
        modules = system / "kernel-modules"
        if not modules.is_symlink():
            raise ValueError("supplied kernel-modules must be a symlink")
        module_image = ModuleImage(root, work)
        module_root = module_image.resolve(modules) / "lib/modules"
        module_root = module_image.resolve(module_root)
        if module_image.inode(module_root)[0] != "directory":
            raise ValueError("system module root must be a directory")
        versions = list(module_root.iterdir())
        if len(versions) != 1 or not versions[0].is_dir():
            raise ValueError("expected exactly one kernel module version directory")
        module_image.compare(module_root)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    command = commands.add_parser("extract")
    command.add_argument("archive", type=Path)
    command.add_argument("destination", type=Path)
    for name in ("populate", "verify-directory", "verify-fat", "verify-image"):
        command = commands.add_parser(name)
        command.add_argument("system", type=Path)
        command.add_argument("loader", type=Path)
        command.add_argument("artifact", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "extract":
            extract(args.archive, args.destination)
        else:
            actions = {
                "populate": populate,
                "verify-directory": verify_directory,
                "verify-fat": verify_fat,
                "verify-image": verify_image,
            }
            actions[args.command](args.system, args.loader, args.artifact)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"recovery: {error}\n")


if __name__ == "__main__":
    main()
