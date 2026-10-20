"""unpack.py <contents> <work-dir> <zip>

The game artifact's unpack program. packages/plugin.nix wraps it as
`mega-man-rock-n-roll-unpack <work-dir> <zip>` with <contents> fixed to the
`contents` file beside this script.

korrid downloads and verifies the ZIP, then runs the program on it (ADR 0012).
korrid owns the work directory, the read-only flip, the sync and the rename
to the ready directory. This program writes only under <work-dir>. It extracts
every regular file and directory, refuses any other entry, and checks each
file that <contents> lists against its SHA-256 value and gives it its mode.
Other files get mode 0644.

Exit status: 0 ready, 1 failed. Diagnostics go to stderr; stdout carries
nothing korrid reads.
"""
import hashlib
import os
from pathlib import Path
import re
import stat
import sys
import zipfile

CHUNK = 1024 * 1024
CONTENTS_LINE = re.compile(r"(0644|0755) sha256:([0-9a-f]{64}) (\S+)")


class Failure(Exception):
    pass


def read_contents(path):
    """Map each listed path to its (mode, sha256 hex)."""
    contents = {}
    for number, line in enumerate(Path(path).read_text().splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        match = CONTENTS_LINE.fullmatch(line)
        if not match:
            raise Failure(f"{path}:{number}: not '<mode> sha256:<hex> <path>'")
        mode, digest, name = match.groups()
        if name in contents or safe_parts(name) is None:
            raise Failure(f"{path}:{number}: duplicate or unsafe path {name!r}")
        contents[name] = (int(mode, 8), digest)
    if not contents:
        raise Failure(f"{path}: lists no files")
    return contents


def safe_parts(name):
    """The path segments of a relative entry name, or None if it could escape."""
    if not name or "\\" in name or "\0" in name or name.startswith("/"):
        return None
    parts = name.rstrip("/").split("/")
    if any(part in ("", ".", "..") for part in parts):
        return None
    return parts


def entry_kind(info):
    """'dir' or 'file'; anything else (a symlink, a device) is refused."""
    unix_type = stat.S_IFMT(info.external_attr >> 16)
    if info.is_dir():
        if unix_type not in (0, stat.S_IFDIR):
            raise Failure(f"entry {info.filename!r} is not a plain directory")
        return "dir"
    if unix_type not in (0, stat.S_IFREG):
        raise Failure(f"entry {info.filename!r} is not a regular file")
    return "file"


def entries(archive):
    """Every entry as (name, parts, kind, info). Refuses the whole archive
    before anything is written if one entry is unsafe."""
    checked = {}
    for info in archive.infolist():
        parts = safe_parts(info.filename)
        if parts is None:
            raise Failure(f"entry {info.filename!r} leaves the work directory")
        name = "/".join(parts)
        if name in checked:
            raise Failure(f"entry {name!r} appears twice")
        checked[name] = (name, parts, entry_kind(info), info)
    return list(checked.values())


def extract(archive, work, contents):
    hashes = {}
    for name, parts, kind, info in entries(archive):
        target = work.joinpath(*parts)
        if kind == "dir":
            target.mkdir(mode=0o755, parents=True, exist_ok=True)
            continue
        target.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
        mode = contents.get(name, (0o644, None))[0]
        # The work directory is new and holds only what this loop made, so no
        # component can be a link; O_EXCL|O_NOFOLLOW still refuses one.
        descriptor = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        digest = hashlib.sha256()
        with os.fdopen(descriptor, "wb") as output, archive.open(info) as source:
            while chunk := source.read(CHUNK):
                digest.update(chunk)
                output.write(chunk)
            os.fchmod(output.fileno(), mode)
        hashes[name] = digest.hexdigest()
    for name, (_, expected) in contents.items():
        if name not in hashes:
            raise Failure(f"archive is missing {name}")
        if hashes[name] != expected:
            raise Failure(f"{name} has sha256:{hashes[name]}, expected sha256:{expected}")


def main(arguments):
    if len(arguments) != 3:
        raise Failure("usage: mega-man-rock-n-roll-unpack <work-dir> <zip>")
    contents_path, work, zip_path = arguments
    work = Path(work)
    if not work.is_absolute() or not work.is_dir() or work.is_symlink():
        raise Failure(f"work directory is not an absolute directory: {work}")
    if not Path(zip_path).is_file():
        raise Failure(f"not a file: {zip_path}")
    contents = read_contents(contents_path)
    try:
        with zipfile.ZipFile(zip_path) as archive:
            extract(archive, work, contents)
    except (zipfile.BadZipFile, zipfile.LargeZipFile, NotImplementedError, RuntimeError) as error:
        raise Failure(f"cannot read {zip_path}: {error}") from error


if __name__ == "__main__":
    os.umask(0o022)
    try:
        main(sys.argv[1:])
    except (Failure, OSError) as error:
        print(f"mega-man-rock-n-roll-unpack: {error}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        sys.exit(130)
