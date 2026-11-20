#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.pyyaml
"""One-off ADR 0090 cut: clean the titles of folder-discovered games.

Stop korrid, inspect --dry-run, then migrate once. Only catalog/library.yaml
changes. device.yaml is read for each release's location and never written.
"""

import argparse
import copy
from dataclasses import dataclass
from datetime import datetime, timezone
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import tempfile

import yaml

class MigrationError(ValueError):
    pass


# --- YAML as serde_yaml reads it (from launch-settings-migration) ----------

INTEGER_LITERAL = re.compile(
    r"^[-+]?(?:0b[01]+|0o[0-7]+|0x[0-9a-fA-F]+|0|[1-9][0-9]*)$"
)


class UniqueLoader(yaml.SafeLoader):
    # serde_yaml reads YAML 1.2 scalars: `yes`, `no`, `on`, `off` stay text.
    yaml_implicit_resolvers = copy.deepcopy(yaml.SafeLoader.yaml_implicit_resolvers)

    def resolve(self, kind, value, implicit):
        tag = super().resolve(kind, value, implicit)
        if kind is yaml.nodes.ScalarNode and tag == "tag:yaml.org,2002:float":
            special = {".inf", "+.inf", "-.inf", ".nan"}
            if value.lower() not in special and not math.isfinite(float(value)):
                return "tag:yaml.org,2002:str"
        return tag


for initial, resolvers in UniqueLoader.yaml_implicit_resolvers.items():
    UniqueLoader.yaml_implicit_resolvers[initial] = [
        (tag, expression)
        for tag, expression in resolvers
        if tag
        not in {
            "tag:yaml.org,2002:bool",
            "tag:yaml.org,2002:timestamp",
            "tag:yaml.org,2002:int",
            "tag:yaml.org,2002:float",
        }
    ]
UniqueLoader.add_implicit_resolver(
    "tag:yaml.org,2002:bool",
    re.compile(r"^(?:true|True|TRUE|false|False|FALSE)$"),
    list("tTfF"),
)
UniqueLoader.add_implicit_resolver(
    "tag:yaml.org,2002:int", INTEGER_LITERAL, list("-+0123456789")
)
UniqueLoader.add_implicit_resolver(
    "tag:yaml.org,2002:float",
    re.compile(
        r"^(?:[-+]?(?:(?:[0-9]+\.[0-9]*|\.[0-9]+)(?:[eE][-+]?[0-9]+)?|[0-9]+[eE][-+]?[0-9]+|\.(?:inf|Inf|INF))|\.(?:nan|NaN|NAN))$"
    ),
    list("-+0123456789."),
)


def unique_mapping(loader, node):
    result = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node)
        if not isinstance(key, str) or key in result:
            raise MigrationError(f"duplicate or non-string YAML key: {key!r}")
        result[key] = loader.construct_object(value_node)
    return result


UniqueLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping
)


def load(text):
    document = yaml.load(text, Loader=UniqueLoader)
    if not isinstance(document, dict):
        raise MigrationError("a configuration document is a record")
    return document


def dump(document):
    return yaml.safe_dump(document, sort_keys=False, allow_unicode=True)


# --- the title rules of services/korrid/src/discovery/title.rs -------------


def without_groups(stem):
    out = []
    paren = bracket = 0
    for character in stem:
        if character == "(" and bracket == 0:
            paren += 1
        elif character == ")" and bracket == 0 and paren > 0:
            paren -= 1
        elif character == "[" and paren == 0:
            bracket += 1
        elif character == "]" and paren == 0 and bracket > 0:
            bracket -= 1
        elif paren == 0 and bracket == 0:
            out.append(character)
    return "".join(out)


def old_fallback_title(stem):
    """main before ADR 0090: `fallback_title` without its extension step."""
    normalized = " ".join(
        "".join(" " if c in "_-." else c for c in without_groups(stem)).split()
    )
    return normalized or " ".join(stem.split())


def invert_article(main):
    for article in ("The", "A", "An"):
        suffix = f", {article}"
        if main.endswith(suffix) and main[: -len(suffix)].strip():
            return f"{article} {main[: -len(suffix)].rstrip()}"
    return main


def clean_title(raw):
    """`discovery::title::clean_title`, rule for rule."""
    spaced = [" " if c in "_." else c for c in without_groups(raw)]

    def blank(index):
        return index < 0 or index >= len(spaced) or spaced[index].isspace()

    separated = "".join(
        " " if c == "-" and not (blank(i - 1) and blank(i + 1)) else c
        for i, c in enumerate(spaced)
    )
    parts = [[]]
    for word in separated.split():
        if word == "-":
            if parts[-1]:
                parts.append([])
        else:
            parts[-1].append(word)
    if not parts[-1]:
        parts.pop()
    parts = [" ".join(words) for words in parts]
    if parts:
        parts[0] = invert_article(parts[0])
    normalized = " - ".join(parts)
    return normalized or " ".join(raw.split())


# --- which titles discovery wrote --------------------------------------------


def location_names(location):
    """The file name discovery took a title from, for a discovered location."""
    if not isinstance(location, dict) or "discovery" not in location:
        return []
    if "files" in location:
        parts = location["files"]
        if not isinstance(parts, list) or not parts:
            return []
        descriptor = [p for p in parts if isinstance(p, dict) and p.get("role") == "descriptor"]
        first = (descriptor or parts)[0]
        path = first.get("path") if isinstance(first, dict) else None
    else:
        path = location.get("path")
    if not isinstance(path, str) or not path:
        return []
    return [PurePosixPath(path).name]


def raw_titles(name):
    """The name without one or two extensions: `x.gba`, `x.p8.png`."""
    stems = []
    stem = name
    for _ in range(2):
        if "." not in stem.lstrip("."):
            break
        stem = stem.rsplit(".", 1)[0]
        stems.append(stem)
    return stems


def discovered_names(game, locations):
    """The file names of the game's discovered releases, oldest first."""
    names = []
    for release in game.get("releases") or []:
        identity = release.get("identity") if isinstance(release, dict) else None
        key = identity.get("value") if isinstance(identity, dict) else None
        if not isinstance(key, str):
            continue
        for location in locations.get(key) or []:
            names.extend(location_names(location))
    return names


def plan(device, library):
    """Each game whose stored title discovery wrote from a file name.

    Discovery wrote the captured `{title}`, a part of the file name, or with
    no capture the old `fallback_title` of the name. A title that is
    neither came from a person, a plugin declaration or acquisition, and
    stays. A title that is already clean stays too, so a second run changes
    nothing.
    """
    locations = device.get("locations") or {}
    games = library.get("library") or {}
    if not isinstance(locations, dict) or not isinstance(games, dict):
        raise MigrationError("locations and library are records")
    changes, kept = {}, {}
    for game_id, game in games.items():
        if not isinstance(game, dict):
            raise MigrationError(f"library.{game_id} is not a record")
        title = game.get("title")
        names = discovered_names(game, locations)
        if not isinstance(title, str) or not names:
            continue
        candidates = [(name, raw) for name in names for raw in raw_titles(name)]
        if any(clean_title(raw) == title for _, raw in candidates):
            continue
        # A layout captures `{title}` from inside the name; the default one
        # captures the whole stem. Without a capture, discovery wrote the old
        # `fallback_title` of the stem.
        captured = next((name for name, raw in candidates if title in raw), None)
        fallback = next(
            ((name, raw) for name, raw in candidates if title == old_fallback_title(raw)),
            None,
        )
        if captured is not None:
            clean, name = clean_title(title), captured
        elif fallback is not None:
            clean, name = clean_title(fallback[1]), fallback[0]
        else:
            kept[game_id] = title
            continue
        if clean != title:
            changes[game_id] = (title, clean, name)
    return changes, kept


def transform(device, library):
    changes, kept = plan(device, library)
    migrated = copy.deepcopy(library)
    for game_id, (_, clean, _) in changes.items():
        migrated["library"][game_id]["title"] = clean
    return migrated, changes, kept


# --- files ----------------------------------------------------------------------


def korrid_running(proc_root=Path("/proc")):
    for entry in proc_root.iterdir():
        if entry.name.isdigit():
            try:
                if (entry / "comm").read_text().strip() == "korrid":
                    return True
            except (FileNotFoundError, PermissionError, ProcessLookupError):
                pass
    return False


POSIX_ACL_ACCESS = "system.posix_acl_access"


@dataclass(frozen=True)
class FileMetadata:
    uid: int
    gid: int
    mode: int
    xattrs: dict


def metadata_from_fd(fd):
    status = os.fstat(fd)
    return FileMetadata(
        status.st_uid,
        status.st_gid,
        stat.S_IMODE(status.st_mode),
        {name: os.getxattr(fd, name) for name in os.listxattr(fd)},
    )


def read_file_metadata(path):
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
        try:
            if not stat.S_ISREG(os.fstat(fd).st_mode):
                raise MigrationError(f"not a regular file: {path}")
            return metadata_from_fd(fd)
        finally:
            os.close(fd)
    except OSError as error:
        raise MigrationError(f"cannot read metadata for {path}: {error}") from error


def apply_file_metadata(fd, expected, destination):
    try:
        current = os.fstat(fd)
        if (current.st_uid, current.st_gid) != (expected.uid, expected.gid):
            os.fchown(fd, expected.uid, expected.gid)
        os.fchmod(fd, 0o600)
        present = {name: os.getxattr(fd, name) for name in os.listxattr(fd)}
        for name in present:
            if name == POSIX_ACL_ACCESS or name not in expected.xattrs:
                os.removexattr(fd, name)
        for name, value in expected.xattrs.items():
            if name != POSIX_ACL_ACCESS and present.get(name) != value:
                os.setxattr(fd, name, value)
        os.fchmod(fd, expected.mode)
        if POSIX_ACL_ACCESS in expected.xattrs:
            os.setxattr(fd, POSIX_ACL_ACCESS, expected.xattrs[POSIX_ACL_ACCESS])
        if metadata_from_fd(fd) != expected:
            raise MigrationError(f"metadata verification failed for {destination}")
    except OSError as error:
        raise MigrationError(
            f"cannot preserve metadata for {destination}: {error}"
        ) from error


def fsync_directory(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def write_copy(fd, data):
    with os.fdopen(fd, "wb", closefd=False) as output:
        output.write(data)
        output.flush()


def backup(root, library_path, original, metadata):
    """Copy library.yaml to a timestamped folder, and prove the copy."""
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    folder = root / f"title-cleaning-backup-{stamp}"
    folder.mkdir(mode=0o700)
    (folder / "catalog").mkdir(mode=0o700)
    destination = folder / "catalog" / "library.yaml"
    fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        write_copy(fd, original)
        apply_file_metadata(fd, metadata, destination)
        os.fsync(fd)
    finally:
        os.close(fd)
    for directory in [folder / "catalog", folder, root]:
        fsync_directory(directory)
    if destination.read_bytes() != original:
        shutil.rmtree(folder)
        raise MigrationError(f"backup {destination} does not match library.yaml")
    if library_path.read_bytes() != original:
        raise MigrationError("library.yaml changed during the migration")
    return folder


def migrate(root, dry_run=False, report=print):
    if korrid_running():
        raise MigrationError("stop korrid before migrating")
    device_path = root / "device.yaml"
    library_path = root / "catalog" / "library.yaml"
    for path in (device_path, library_path):
        if path.is_symlink() or not path.is_file():
            raise MigrationError(f"expected a regular file: {path}")
    original = library_path.read_bytes()
    device = load(device_path.read_text())
    library = load(original.decode())
    migrated, changes, kept = transform(device, library)
    for game_id, (old, new, name) in sorted(changes.items()):
        report(f"{game_id}: {old!r} -> {new!r} (from {name!r})")
    for game_id, title in sorted(kept.items()):
        report(f"{game_id}: kept {title!r}: discovery did not write it")
    if dry_run or not changes:
        return changes, None
    output = dump(migrated)
    if load(output) != migrated:
        raise MigrationError("the written YAML would not read back as the migration")
    metadata = read_file_metadata(library_path)
    folder = backup(root, library_path, original, metadata)
    fd, name = tempfile.mkstemp(prefix=".title-cleaning-", dir=library_path.parent)
    temporary = Path(name)
    try:
        apply_file_metadata(fd, metadata, temporary)
        with os.fdopen(fd, "w", closefd=False, encoding="utf-8") as handle:
            handle.write(output)
            handle.flush()
            apply_file_metadata(fd, metadata, temporary)
            os.fsync(fd)
        if library_path.read_bytes() != original:
            raise MigrationError("library.yaml changed during the migration")
        os.replace(temporary, library_path)
        fsync_directory(library_path.parent)
    finally:
        os.close(fd)
        temporary.unlink(missing_ok=True)
    return changes, folder


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--storage-root", type=Path, required=True)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    try:
        changes, folder = migrate(args.storage_root, args.dry_run)
    except (MigrationError, OSError, UnicodeDecodeError, yaml.YAMLError) as error:
        parser.exit(1, f"migration refused: {error}\n")
    if args.dry_run:
        print(f"{len(changes)} titles would change")
    else:
        print(f"{len(changes)} titles changed" if changes else "already migrated")
    if folder:
        print(f"backup: {folder}")


if __name__ == "__main__":
    main()
