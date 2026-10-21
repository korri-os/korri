"""One-off, offline moves. No daemon fallback or migration state is written."""

import argparse
import ctypes
import json
import os
import re
import stat
import sys
from pathlib import Path
from urllib.parse import quote


def encode_component(value: str) -> str:
    # Exact codec from services/korrid/src/host/play_log.rs, not quote's '/' default.
    return quote(value, safe="-_.!~*'()")


def checked_path(path: Path) -> Path:
    """Reject symlinks and non-directory ancestors, including dangling links."""
    if not path.is_absolute() or ".." in path.parts:
        raise ValueError(f"Expected an absolute path without '..': {path}")
    for component in [*reversed(path.parents), path]:
        try:
            mode = component.lstat().st_mode
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(mode):
            raise ValueError(f"Refusing symlink: {component}")
        if component != path and not stat.S_ISDIR(mode):
            raise ValueError(f"Not a directory: {component}")
    return path


def require_directory(path: Path) -> Path:
    checked_path(path)
    if not path.is_dir():
        raise ValueError(f"Missing directory: {path}")
    return path


def require_idle(private: Path) -> None:
    require_directory(private)
    journal = checked_path(private / "host-session")
    # Fail closed for running, frozen, completionPending, corrupt and temporary
    # journals. Only korrid can reconcile these; an operator must not delete one.
    if journal.exists() and (not journal.is_dir() or any(journal.iterdir())):
        raise ValueError(
            "Session journal is not empty; end/reconcile sessions with the old korrid first"
        )


def read_regular(path: Path) -> str:
    checked_path(path)
    if not path.is_file():
        raise ValueError(f"Not a regular file: {path}")
    return path.read_text(encoding="utf-8")


def owner_account(storage: Path, private: Path) -> Path:
    # identity.rs: persisted, daemon-validated NIP-78 owner event. Do not invoke
    # 'identity status': load_or_create can write keys, even during a dry run.
    event = json.loads(read_regular(private / "identity/owner.event.json"))
    owner = event.get("pubkey")
    tags = event.get("tags")
    if not isinstance(owner, str) or not re.fullmatch(r"[0-9a-f]{64}", owner):
        raise ValueError("Owner event has no valid public key")
    if (
        event.get("kind") != 30078
        or event.get("content") != ""
        or not isinstance(tags, list)
        or len(tags) != 3
        or not isinstance(tags[1], list)
        or len(tags[1]) != 2
        or tags[1][0] != "device"
        or not isinstance(tags[1][1], str)
        or not re.fullmatch(r"[0-9a-f]{64}", tags[1][1])
        or tags[0] != ["d", f"org.korri.device-owner:{tags[1][1]}"]
        or tags[2] != ["status", "owned"]
    ):
        raise ValueError("Owner event is not an owned device binding")
    return checked_path(storage / "users" / encode_component(owner))


def account_moves(storage: Path, account: Path) -> list[tuple[Path, Path]]:
    source = checked_path(storage / "users/default")
    if not source.exists():
        require_directory(account)  # rerunning a completed migration is safe
        return []
    require_directory(source)
    return [(source, account)]


def catalog_stems(storage: Path) -> dict[str, set[str]]:
    import yaml

    class UniqueLoader(yaml.SafeLoader):
        pass

    def unique_mapping(loader, node):
        result = {}
        for key_node, value_node in node.value:
            key = loader.construct_object(key_node)
            if key in result:
                raise ValueError("Duplicate catalog YAML key")
            result[key] = loader.construct_object(value_node)
        return result

    UniqueLoader.add_constructor(
        yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping
    )
    # Existing producers: config/library.rs and config/mod.rs Location::File.
    try:
        library = yaml.load(
            read_regular(storage / "catalog/library.yaml"), Loader=UniqueLoader
        )
        device = yaml.load(read_regular(storage / "device.yaml"), Loader=UniqueLoader)
    except yaml.YAMLError as error:
        raise ValueError("Catalog YAML is malformed") from error
    if not isinstance(library, dict) or not isinstance(device, dict):
        raise TypeError("Catalog documents must be mappings")
    stems: dict[str, set[str]] = {}
    for game, item in library.get("library", {}).items():
        if (
            not isinstance(game, str)
            or not re.fullmatch(r"[a-z0-9][a-z0-9_.-]*", game)
            or len(encode_component(game)) > 255
        ):
            raise ValueError("Invalid catalog game id")
        for release in item["releases"]:
            identity = release.get("identity")
            if identity is None:
                continue
            for location in device.get("locations", {}).get(identity["value"], []):
                # Provider, executable and multi-file locations are not this GBA slice.
                if "storage" not in location or "path" not in location:
                    continue
                path = location["path"]
                if (
                    not isinstance(path, str)
                    or not path
                    or Path(path).is_absolute()
                    or ".." in Path(path).parts
                ):
                    raise ValueError("Unsafe catalog content path")
                stem = Path(path).stem
                stems.setdefault(stem, set()).add(game)
    return stems


def location_files(root: Path, recursive: bool):
    checked_path(root)
    if not root.exists():
        return
    require_directory(root)
    for path in sorted(root.iterdir()):
        checked_path(path)
        if path.is_dir():
            if recursive:
                yield from location_files(path, True)
        elif path.is_file():
            yield path
        else:
            raise ValueError(f"Refusing special file: {path}")


def retroarch_moves(storage: Path, account: Path) -> list[tuple[Path, Path]]:
    require_directory(account)
    stems = catalog_stems(storage)
    moves = []
    for location in ("saves", "states", "screenshots"):
        root = account / location
        for path in location_files(root, location == "states"):
            games: set[str] = set()
            for stem, candidates in stems.items():
                if location == "saves":
                    matches = path.name == f"{stem}.srm"
                elif location == "states":
                    matches = path.parent != root and path.name.startswith(f"{stem}.")
                else:
                    # RetroArch task_screenshot.c + fill_str_dated_filename:
                    # name_base.png or name_base-YYMMDD-HHMMSS.{png,bmp}.
                    matches = (
                        re.fullmatch(
                            re.escape(stem) + r"(?:-\d{6}-\d{6})?\.(?:png|bmp)",
                            path.name,
                        )
                        is not None
                    )
                if matches:
                    games.update(candidates)
            if len(games) != 1:
                label = "AMBIGUOUS" if games else "UNMATCHED"
                print(f"{label} (left in place): {str(path)!r}")
                continue
            game = next(iter(games))
            target = (
                account
                / "saves"
                / encode_component(game)
                / location
                / path.relative_to(root)
            )
            moves.append((path, target))
    return moves


def preflight(moves: list[tuple[Path, Path]]) -> None:
    destinations = set()
    for source, target in moves:
        checked_path(source)
        checked_path(target)
        if target.exists() or target in destinations:
            raise ValueError(f"Destination already exists; no files moved: {target}")
        destinations.add(target)


def make_parents(path: Path, template: os.stat_result) -> None:
    checked_path(path)
    if path.exists():
        require_directory(path)
        return
    make_parents(path.parent, template)
    path.mkdir(mode=0o700)
    current = path.stat()
    if (current.st_uid, current.st_gid) != (template.st_uid, template.st_gid):
        os.chown(path, template.st_uid, template.st_gid)
    path.chmod(stat.S_IMODE(template.st_mode))


def rename_without_replace(source: Path, target: Path) -> None:
    # Linux renameat2 preserves files/directories and refuses a collision even
    # after preflight. No cross-filesystem copy or overwrite fallback.
    libc = ctypes.CDLL(None, use_errno=True)
    rename = libc.renameat2
    rename.argtypes = [
        ctypes.c_int,
        ctypes.c_char_p,
        ctypes.c_int,
        ctypes.c_char_p,
        ctypes.c_uint,
    ]
    rename.restype = ctypes.c_int
    if rename(-100, os.fsencode(source), -100, os.fsencode(target), 1) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), str(target))
    for directory in {source.parent, target.parent}:
        descriptor = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def run(kind: str) -> int:
    parser = argparse.ArgumentParser(
        description="Offline save migration. End sessions and stop korrid first."
    )
    parser.add_argument("--storage-root", required=True, type=Path)
    parser.add_argument("--private-state-root", required=True, type=Path)
    parser.add_argument(
        "--dry-run", action="store_true", help="list moves without writing anything"
    )
    args = parser.parse_args()
    try:
        storage = require_directory(args.storage_root)
        require_idle(args.private_state_root)
        account = owner_account(storage, args.private_state_root)
        plan = (
            account_moves(storage, account)
            if kind == "account"
            else retroarch_moves(storage, account)
        )
        preflight(plan)
        for source, target in plan:
            print(f"MOVE {str(source)!r} -> {str(target)!r}")
        if not args.dry_run:
            for source, target in plan:
                require_idle(args.private_state_root)
                checked_path(source)
                checked_path(target)
                make_parents(target.parent, source.parent.stat())
                rename_without_replace(source, target)
        return 0
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        print(f"Migration refused: {error}", file=sys.stderr)
        return 1
