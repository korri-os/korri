"""Explicit repair of the shipped identity switch's stranded save history."""

import hashlib
import json
import re
import stat
from datetime import datetime, timezone
from pathlib import Path

from migration import (
    checked_path,
    encode_component,
    location_files,
    read_regular,
    require_directory,
)


KEY = re.compile(r"[0-9a-f]{64}")


def fingerprint(path: Path) -> tuple[int, str]:
    checked_path(path)
    if not path.is_file() or path.stat().st_nlink != 1:
        raise ValueError(f"Expected a single-link regular file: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(65536), b""):
            digest.update(block)
    return path.stat().st_size, digest.hexdigest()


def one_tree(source: Path, target: Path) -> Path:
    checked_path(source)
    checked_path(target)
    if source.exists() and target.exists():
        raise ValueError(f"Both identity trees exist; refusing to merge: {target}")
    return require_directory(source if source.exists() else target)


def adoption_moves(
    storage: Path, private: Path, account: Path, person: str, signer_root: Path
) -> list[tuple[Path, Path]]:
    # Owner decision 2026-10-22, ADR "An owned device joins by identity switch,
    # keeping its data" (0023, docs/device-join-design). The shipped coordinator
    # moved only play-log records. This is a consented offline repair, not a
    # rule that another person's saves automatically belong to the current owner.
    if not KEY.fullmatch(person) or person == account.name:
        raise ValueError("Adoption needs a different retired person public key")
    transaction = checked_path(private / "identity-switch")
    if transaction.exists():
        require_directory(transaction)
        if any(transaction.iterdir()):
            raise ValueError(
                "Identity switch is unfinished; recover with the old korrid"
            )
    if checked_path(storage / "users/default").exists():
        raise ValueError(
            "Cannot combine default-account migration with identity adoption"
        )

    identity = require_directory(signer_root / "identity")
    active = require_directory(identity / "active")
    retired = require_directory(identity / "retired")
    if any(active.iterdir()):
        raise ValueError("Local signer still has an active key")
    if sorted(p.name for p in retired.iterdir()) != [person]:
        raise ValueError("Expected exactly the named retired person key")
    key_directory = require_directory(retired / person)
    if sorted(p.name for p in key_directory.iterdir()) != ["person.key"]:
        raise ValueError("Retired person key directory is incomplete or unexpected")
    retired_key = checked_path(key_directory / "person.key")
    metadata = retired_key.stat()
    if (
        not stat.S_ISREG(metadata.st_mode)
        or metadata.st_nlink != 1
        or stat.S_IMODE(metadata.st_mode) != 0o600
        or metadata.st_size != 65
    ):
        raise ValueError("Retired person key is not a private signer record")
    # Like owner_account, trust the previously daemon/signer-validated private
    # record. Never read, copy, hash or print the person private key.
    owner = json.loads(read_regular(private / "identity/owner.event.json"))
    connection = json.loads(read_regular(private / "identity/nip46.connection.json"))
    if connection.get("userPublicKey") != account.name:
        raise ValueError("NIP-46 person does not match the device owner")
    device = owner["tags"][1][1]
    owner_time = owner["created_at"]
    if type(owner_time) is not int:
        raise ValueError("Owner statement has no creation time")

    old_account = checked_path(storage / "users" / person)
    old_history = checked_path(private / "saves" / person)
    new_history = checked_path(private / "saves" / account.name)
    source_account = one_tree(old_account, account)
    source_history = one_tree(old_history, new_history)
    account_files = set(location_files(source_account, True))
    history_files = set(location_files(source_history, True))
    if not history_files:
        raise ValueError("No snapshot history proves the identity switch")
    objects: set[Path] = set()
    records: dict[str, dict[str, dict]] = {}
    for path in sorted(history_files):
        relative = path.relative_to(source_history)
        if (
            len(relative.parts) != 2
            or not KEY.fullmatch(path.stem)
            or path.suffix != ".json"
        ):
            raise ValueError(
                f"Unsupported history file; review before adoption: {path}"
            )
        _, digest = fingerprint(path)
        if digest != path.stem:
            raise ValueError(f"Snapshot hash does not match its record: {path}")
        record = json.loads(read_regular(path))
        game = record["gameId"]
        if encode_component(game) != relative.parts[0]:
            raise ValueError(f"Snapshot game does not match its directory: {path}")
        if not KEY.fullmatch(record["device"]) or record["device"] == device:
            raise ValueError("Snapshot does not name a previous device identity")
        ended = datetime.strptime(record["endedAt"], "%Y-%m-%dT%H:%M:%S.%fZ").replace(
            tzinfo=timezone.utc
        )
        if ended.timestamp() >= owner_time:
            raise ValueError("Snapshot is not older than the identity switch")
        log = json.loads(
            read_regular(
                private / "play-log" / account.name / f"{relative.parts[0]}.json"
            )
        )
        if (
            log.get("userId") != account.name
            or log.get("gameId") != game
            or {
                "occurredAt": record["endedAt"],
                "durationSeconds": record["durationSeconds"],
            }
            not in log["entries"]
        ):
            raise ValueError(
                "Transferred play log does not prove the snapshot's session"
            )
        records.setdefault(game, {})[f"sha256:{digest}"] = record
        for location in record["locations"]:
            for file in location["files"]:
                hash_value = file["sha256"]
                if not hash_value.startswith("sha256:") or not KEY.fullmatch(
                    hash_value[7:]
                ):
                    raise ValueError("Snapshot object hash is invalid")
                obj = private / "saves/objects" / hash_value[7:]
                if fingerprint(obj) != (file["bytes"], hash_value[7:]):
                    raise ValueError(f"Snapshot object bytes/hash differ: {obj}")
                objects.add(obj)

    expected_live: dict[Path, tuple[int, str]] = {}
    for game, history in records.items():
        parents = {
            parent for record in history.values() for parent in record["parents"]
        }
        if parents - history.keys():
            raise ValueError("Snapshot ancestry is incomplete; review before adoption")
        heads = history.keys() - parents
        if len(heads) != 1:
            raise ValueError(
                "Snapshot history is split or cyclic; review before adoption"
            )
        head = history[next(iter(heads))]
        for location in head["locations"]:
            key = location["key"]
            if not key or key in (".", "..") or "/" in key:
                raise ValueError("Snapshot location key is unsafe")
            for file in location["files"]:
                relative = Path(file["path"])
                if not file["path"] or relative.is_absolute() or ".." in relative.parts:
                    raise ValueError("Snapshot file path is unsafe")
                live = (
                    source_account / "saves" / encode_component(game) / key / relative
                )
                if live in expected_live:
                    raise ValueError("Snapshot lists a live file more than once")
                expected_live[live] = (file["bytes"], file["sha256"][7:])
    live_files = set(location_files(source_account / "saves", True))
    if live_files != expected_live.keys():
        raise ValueError("Live saves are not exactly the snapshot head's files")
    for path, expected in expected_live.items():
        if fingerprint(path) != expected:
            raise ValueError(f"Live save differs from its snapshot: {path}")
    # A previous play log at the retired identity makes the inferred transfer
    # ambiguous. Do not silently combine independently used person histories.
    old_log = checked_path(private / "play-log" / person)
    if old_log.exists():
        raise ValueError(
            "Retired person's play log still exists; transfer is not proven"
        )

    plan = []
    if source_account == old_account:
        plan.append((old_account, account))
    if source_history == old_history:
        plan.append((old_history, new_history))
    print(
        f"ADOPTION consented retired={person} owner={account.name} owner-event={owner['id']} device={device}"
    )
    print(
        f"EVIDENCE retired-key={str(retired_key)!r} (signer-validated; private bytes not read); active empty; NIP-46 owner matches"
    )
    print(
        "EVIDENCE snapshot sessions match transferred owner play-log; complete ancestry and objects; live saves match head"
    )
    for game, history in sorted(records.items()):
        log_path = (
            private / "play-log" / account.name / f"{encode_component(game)}.json"
        )
        for snapshot, record in sorted(history.items()):
            print(
                f"EVIDENCE snapshot={snapshot} play-log={str(log_path)!r} "
                f"endedAt={record['endedAt']} durationSeconds={record['durationSeconds']} "
                f"previous-device={record['device']} owner-created-at={owner_time}"
            )
    for root, target, files in (
        (source_account, account, account_files),
        (source_history, new_history, history_files),
        (private / "saves/objects", private / "saves/objects", objects),
    ):
        for path in sorted(files):
            size, digest = fingerprint(path)
            destination = target / path.relative_to(root)
            action = "RETAINED" if path == destination else "MOVE"
            print(
                f"FILE {action} {str(path)!r} -> {str(destination)!r} bytes={size} sha256={digest}"
            )
    print(
        f"ADOPTION CLASSIFIED total={len(account_files) + len(history_files) + len(objects)} live-saves={len(live_files)} snapshots={len(history_files)} objects={len(objects)} unclassified=0"
    )
    return plan
