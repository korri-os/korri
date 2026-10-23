"""One-off, offline moves. No daemon fallback or migration state is written."""

import argparse
import ctypes
import json
import os
import pwd
import re
import stat
import struct
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


# Audited cartdata() calls in the unchanged Starter pack PNG payload. Keys are
# existing catalog release identities, never cart filenames or game IDs. See
# README.md for source pins, Lua extraction and the consumer path proof.
FAKE08_CARTDATA = {
    "sha256:310a57c27b0c2a49a6cfe68f0a9fddf5ebcb4b35be6f385c1695da5b32beffcd": "i_k_wormnomnom",
    "sha256:07c1155f63a0a13e6300ef9aef8a26b0df650f15eb24a9ed73bb68394f079c0a": "toad_spaceman8",
    "sha256:9e2c7759826af559697e0c69a331c8a14db2ddd5b1108bc91ea9fbe1570706d1": "nusan_defacto",
    "sha256:ba3250ad0f3e73844d7003139ec0f46efe708d4764bac7d55075f851ddbfe1de": "sweetbuns1",
    "sha256:e6b223cc8d91fded83605444cf9ef68366e88a987713e5c6bbbf4260163e51e3": "cherrybomb",
    "sha256:a5cff876236fdc4a225684b43f5acf92cb8e9e06bc12de819dcac521233c3ab6": "highstakes",
    "sha256:9d98592e889a7e8a6efaac2534f09f70b3770d96b521911c36adfefe346f0c07": "2darray_marble_merger",
    "sha256:f42ffd0cfb87cc24d9eb26f92bd4553d46b1b2bef5c6ee201e5b9665b23cac8f": "mot_whiplashtaxi",
    "sha256:6022f3e36884e02ea851f35663a6ebeef0bde51d2146a65ec1d541fc8d1b9f56": "disc",
    "sha256:cacfc19bd8aaa6e7f19aea36e5c8c726e43a895e45a8a208912ecb18699862fe": "mush_vitreous_1",
}


def catalog_matches(
    storage: Path,
) -> tuple[dict[str, set[str]], dict[str, set[str]], set[str]]:
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
    cartdata: dict[str, set[str]] = {}
    games: set[str] = set()
    for game, item in library.get("library", {}).items():
        if (
            not isinstance(game, str)
            or not re.fullmatch(r"[a-z0-9][a-z0-9_.-]*", game)
            or len(encode_component(game)) > 255
        ):
            raise ValueError("Invalid catalog game id")
        games.add(game)
        for release in item["releases"]:
            identity = release.get("identity")
            if identity is None:
                continue
            if release["system"] == "pico8" and identity["kind"] == "hash":
                data_id = FAKE08_CARTDATA.get(identity["value"])
                if data_id is not None:
                    cartdata.setdefault(data_id, set()).add(game)
            for location in device.get("locations", {}).get(identity["value"], []):
                # Provider, executable and multi-file locations have no file stem.
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
    return stems, cartdata, games


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


def check_classification(
    files: set[Path],
    moves: list[tuple[Path, Path]],
    retained: list[Path],
    unresolved: list[Path],
) -> None:
    classified = [source for source, _ in moves] + retained + unresolved
    if len(classified) != len(set(classified)):
        raise ValueError("File classified more than once; no files moved")
    missing = files - set(classified)
    if missing:
        raise ValueError(
            f"Unclassified files; no files moved: {sorted(map(str, missing))}"
        )
    if set(classified) - files:
        raise ValueError("Report includes files outside the inventory; no files moved")
    print(
        f"CLASSIFIED total={len(files)} moved={len(moves)} retained={len(retained)} "
        f"unresolved={len(unresolved)} unclassified=0"
    )


def retroarch_moves(storage: Path, account: Path) -> list[tuple[Path, Path]]:
    require_directory(account)
    stems, cartdata, library_games = catalog_matches(storage)
    moves: list[tuple[Path, Path]] = []
    retained: list[Path] = []
    unresolved: list[Path] = []
    games_encoded = {encode_component(game) for game in library_games}
    # Inventory recursively, independently of which old layouts we understand.
    # An unknown nested file must be unresolved, never absent from the report.
    inventory = {
        location: set(location_files(account / location, True))
        for location in ("saves", "states", "screenshots")
    }
    for location, files in inventory.items():
        root = account / location
        for path in sorted(files):
            relative = path.relative_to(root)
            if (
                location == "saves"
                and len(relative.parts) > 1
                and relative.parts[0] in games_encoded
            ):
                retained.append(path)
                print(f"RETAINED (per-game root): {str(path)!r}")
                continue
            games: set[str] = set()
            if (
                location == "saves"
                and relative.parts[:2] == ("fake-08", "cdata")
                and len(relative.parts) == 3
                and path.name.endswith(".p8d.txt")
            ):
                # Host::getCartDataFile uses the Lua cartdata key, not the
                # cartridge stem. Keep fake-08/cdata exactly as the core reads it.
                games.update(cartdata.get(path.name.removesuffix(".p8d.txt"), set()))
            else:
                for stem, candidates in stems.items():
                    if location == "saves":
                        matches = (
                            len(relative.parts) <= 2 and path.name == f"{stem}.srm"
                        )
                    elif location == "states":
                        matches = path.parent != root and path.name.startswith(
                            f"{stem}."
                        )
                    else:
                        # RetroArch task_screenshot.c + fill_str_dated_filename.
                        matches = (
                            len(relative.parts) == 1
                            and re.fullmatch(
                                re.escape(stem) + r"(?:-\d{6}-\d{6})?\.(?:png|bmp)",
                                path.name,
                            )
                            is not None
                        )
                    if matches:
                        games.update(candidates)
            if len(games) != 1:
                unresolved.append(path)
                label = "AMBIGUOUS" if games else "UNMATCHED"
                print(
                    f"{label} (left in place): {str(path)!r}; games={sorted(games)!r}"
                )
                continue
            game = next(iter(games))
            target = account / "saves" / encode_component(game) / location / relative
            moves.append((path, target))
    check_classification(set().union(*inventory.values()), moves, retained, unresolved)
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


def save_access_paths(root: Path) -> list[Path]:
    checked_path(root)
    if not root.exists():
        return []
    require_directory(root)
    paths = [root]
    for path in sorted(root.iterdir()):
        checked_path(path)
        if path.is_dir():
            paths.extend(save_access_paths(path))
        elif path.is_file():
            # An ACL changes the inode, including links outside saves/.
            if path.stat().st_nlink != 1:
                raise ValueError(f"Refusing hard-linked save file: {path}")
            paths.append(path)
        else:
            raise ValueError(f"Refusing special file: {path}")
    return paths


def grant_save_access(
    root: Path, daemon_uid: int, runtime_uid: int, *, dry_run: bool = False
) -> None:
    paths = save_access_paths(root)
    print(f"ACL (recursive access, directory defaults): {str(root)!r}")
    if dry_run:
        return
    for path in paths:
        checked_path(path)
        directory = path.is_dir()
        permissions = 7 if directory or path.stat().st_mode & 0o111 else 6
        # services/korrid/src/saves/access.rs::prepare_account: shared ACL on
        # saves/ only. Named daemon/runtime users and mask get rwx; group and
        # other get no access. Files use rwX, not unconditional execute. Keep
        # the person root and all runner siblings unchanged (ADR 0014 / #139).
        # Like the other migration effects, use native filesystem calls. The
        # product references pkgs.acl in helpers, not as a system package.
        entries = [(1, permissions, 0xFFFFFFFF)]
        entries.extend(
            (2, permissions, uid) for uid in sorted({daemon_uid, runtime_uid})
        )
        entries.extend(
            [(4, 0, 0xFFFFFFFF), (16, permissions, 0xFFFFFFFF), (32, 0, 0xFFFFFFFF)]
        )
        acl = struct.pack("<I", 2) + b"".join(
            struct.pack("<HHI", *entry) for entry in entries
        )
        os.setxattr(path, "system.posix_acl_access", acl, follow_symlinks=False)
        if directory:
            os.setxattr(path, "system.posix_acl_default", acl, follow_symlinks=False)


def run(kind: str, save_users: tuple[int, int] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Offline save migration. End sessions and stop korrid first."
    )
    parser.add_argument("--storage-root", required=True, type=Path)
    parser.add_argument("--private-state-root", required=True, type=Path)
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="list moves and ACL changes without writing anything",
    )
    if kind == "account":
        parser.add_argument(
            "--adopt-retired-person",
            metavar="RETIRED_PERSON:OWNER",
            help="consent to repair only the captured SP switch between these public keys",
        )
        parser.add_argument("--local-signer-state-root", type=Path)
    args = parser.parse_args()
    try:
        storage = require_directory(args.storage_root)
        require_idle(args.private_state_root)
        account = owner_account(storage, args.private_state_root)
        if kind == "account" and args.adopt_retired_person:
            from adoption import adoption_moves

            if args.local_signer_state_root is None:
                raise ValueError("Adoption requires --local-signer-state-root")
            plan = adoption_moves(
                storage,
                args.private_state_root,
                account,
                args.adopt_retired_person,
                args.local_signer_state_root,
            )
        else:
            if kind == "account" and args.local_signer_state_root is not None:
                raise ValueError(
                    "Signer root requires explicit --adopt-retired-person consent"
                )
            plan = (
                account_moves(storage, account)
                if kind == "account"
                else retroarch_moves(storage, account)
            )
        preflight(plan)
        if kind == "retroarch":
            if save_users is None:
                try:
                    save_users = (
                        pwd.getpwnam("korrid").pw_uid,
                        pwd.getpwnam("korri").pw_uid,
                    )
                except KeyError as error:
                    raise ValueError(
                        f"Required product account is missing: {error}"
                    ) from error
            save_access_paths(account / "saves")  # fail before any moves
            for source, _ in plan:
                if source.stat().st_nlink != 1:
                    raise ValueError(f"Refusing hard-linked save file: {source}")
        for source, target in plan:
            print(f"MOVE {str(source)!r} -> {str(target)!r}")
        if not args.dry_run:
            for source, target in plan:
                require_idle(args.private_state_root)
                checked_path(source)
                checked_path(target)
                make_parents(target.parent, source.parent.stat())
                rename_without_replace(source, target)
        if kind == "retroarch":
            require_idle(args.private_state_root)
            assert save_users is not None
            grant_save_access(account / "saves", *save_users, dry_run=args.dry_run)
        return 0
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        print(f"Migration refused: {error}", file=sys.stderr)
        return 1
