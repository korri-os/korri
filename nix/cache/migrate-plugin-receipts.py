#!/usr/bin/env -S nix shell nixpkgs#python3 nixpkgs#nix -c python3
"""One-off, offline cache-URL cutover for the existing plugin selection format.

Grounding: plugin-host/src/selection.rs owns Receipt; package.rs owns
PublisherBindings; `korri-plugin seed-graph` owns graph approvals. This script
adds no host schema or runtime fallback. Stop the portal, korrid and the plugin
admin/restore services before applying. Restore bindings AND receipts on rollback.
"""

import argparse
from contextlib import contextmanager
import fcntl
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile


NAME = re.compile(r"korri-plugin-[0-9a-f]{64}")
FIELDS = {"id", "package", "provenance", "approval", "desired", "previous"}
# The host owns these state-root entries. plugin-host/src/storage.rs names them
# SOURCES_DIR and STAGING_DIR. The migration never reads or writes them, and a
# real device always has at least an empty sources directory, so refusing them
# would refuse every device. Each must still be a real directory, never a
# symlink, and any other entry keeps its refusal.
HOST_DIRS = {"sources", "staging"}


def regular(path):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError(f"expected regular file: {path}")
    return path.read_bytes()


def directory(path):
    if not stat.S_ISDIR(path.lstat().st_mode):
        raise ValueError(f"expected directory: {path}")


def command(*args):
    return subprocess.check_output(list(map(str, args)), text=True).strip()


def receipts(root):
    directory(root)
    selected = {}
    for folder in root.iterdir():
        if folder.name == "graph-update.json":
            raise ValueError("unfinished graph update; recover before migration")
        if not NAME.fullmatch(folder.name):
            if folder.name in HOST_DIRS:
                directory(folder)
                continue
            if folder.name != "lock":
                raise ValueError(f"unexpected plugin state entry: {folder.name}")
            continue
        directory(folder)
        path = folder / "selection.json"
        if not path.exists() and not path.is_symlink():
            raise ValueError(f"unselected plugin directory needs cleanup: {folder}")
        selected[folder.name] = regular(path)
    if not selected:
        raise ValueError("no plugin selections")
    return selected


@contextmanager
def locked(state):
    directory(state)
    descriptor = os.open(state / "lock", os.O_RDWR | os.O_NOFOLLOW)
    try:
        fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield
    finally:
        os.close(descriptor)


def settled(roots, snapshot):
    directory(roots)
    for name, data in snapshot.items():
        folder = roots / name
        directory(folder)
        receipt = json.loads(data)
        for path in folder.iterdir():
            if path.name not in {"active", "previous"}:
                raise ValueError(f"unfinished plugin operation: {path}")
        active = folder / "active"
        if not active.is_symlink() or os.readlink(active) != receipt["package"]:
            raise ValueError(f"active root differs from receipt: {name}")
        previous = folder / "previous"
        if receipt["previous"] is None:
            if previous.exists() or previous.is_symlink():
                raise ValueError(f"unexpected previous root: {name}")
        elif (
            not previous.is_symlink()
            or os.readlink(previous) != receipt["previous"]["package"]
        ):
            raise ValueError(f"previous root differs from receipt: {name}")


def atomic_write(path, data):
    descriptor, temporary = tempfile.mkstemp(prefix=".cache-cutover-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as target:
            target.write(data)
            target.flush()
            os.fsync(target.fileno())
        os.replace(temporary, path)
        parent = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(parent)
        finally:
            os.close(parent)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def changed_approvals(before, after):
    return {
        json.loads(after[name])["id"]: json.loads(after[name])["approval"]
        for name in before
        if json.loads(before[name]) != json.loads(after[name])
    }


def prepare(args):
    # The only durable records here are unmodified/current Receipt values and
    # the existing PublisherBindings format. There is no migration manifest.
    old_bytes, new_bytes = regular(args.bindings_before), regular(args.bindings_after)
    old, new = json.loads(old_bytes), json.loads(new_bytes)
    if any(
        namespace not in new or binding["publicKey"] != new[namespace]["publicKey"]
        for namespace, binding in old.items()
    ):
        raise ValueError("cache cutover must not revoke namespaces or rotate keys")
    with locked(args.state):
        before = receipts(args.state)
        settled(args.roots, before)
        selections = [json.loads(data) for data in before.values()]
        if any(
            set(r) != FIELDS
            or r["desired"]["state"] not in {"Enabled", "Disabled"}
            or r["provenance"]["kind"] != "RawCache"
            for r in selections
        ):
            raise ValueError("migration accepts only settled raw-cache Receipt values")
        for name, data in before.items():
            receipt = json.loads(data)
            if command(args.host, "unit-name", receipt["id"]) != name:
                raise ValueError("receipt directory does not match its identity")
            namespace = receipt["id"].split(":", 1)[0]
            if receipt["provenance"]["cache_url"] != old[namespace]["cacheUrl"]:
                raise ValueError("receipt does not match the old bound cache")
            # Native verification accepts content-addressed paths without a
            # signature. This one-off cutover is only for the publisher's
            # existing input-addressed outputs; do not substitute CA trust.
            info = json.loads(
                command(
                    args.nix,
                    "path-info",
                    "--json",
                    "--json-format",
                    "1",
                    receipt["package"],
                )
            )[receipt["package"]]
            if info["ca"] is not None:
                raise ValueError(
                    "migration requires input-addressed signed plugin outputs"
                )
            # Same full publisher key, not any key trusted by the machine.
            command(
                args.nix,
                "--extra-experimental-features",
                "nix-command",
                "--option",
                "trusted-public-keys",
                old[namespace]["publicKey"],
                "--option",
                "extra-trusted-public-keys",
                "",
                "store",
                "verify",
                "--sigs-needed",
                "1",
                receipt["package"],
            )
        disabled = [
            word
            for r in selections
            if r["desired"]["state"] == "Disabled"
            for word in ("--disabled", r["id"])
        ]
        packages = [r["package"] for r in selections]

        def seed(bindings):
            return {
                r["id"]: r
                for r in json.loads(
                    command(args.host, "seed-graph", *disabled, bindings, *packages)
                )
            }

        prior, next_graph = seed(args.bindings_before), seed(args.bindings_after)
        after = {}
        for name, data in before.items():
            receipt = json.loads(data)
            expected = dict(receipt, previous=None)
            if prior.get(receipt["id"]) != expected:
                raise ValueError(f"existing approval/graph is stale: {receipt['id']}")
            replacement = dict(next_graph[receipt["id"]], previous=receipt["previous"])
            if (
                replacement["package"] != receipt["package"]
                or replacement["desired"] != receipt["desired"]
            ):
                raise ValueError(
                    "migration must preserve selections and enabled states"
                )
            after[name] = (json.dumps(replacement, indent=2) + "\n").encode()
        if receipts(args.state) != before:
            raise ValueError("selections changed during preparation")
        args.backup.mkdir(mode=0o700)
        for side, snapshot in (("before", before), ("after", after)):
            root = args.backup / side
            root.mkdir(mode=0o700)
            for name, data in snapshot.items():
                (root / name).mkdir(mode=0o700)
                atomic_write(root / name / "selection.json", data)
        atomic_write(args.backup / "publishers.before.json", old_bytes)
        atomic_write(args.backup / "publishers.after.json", new_bytes)
        print(json.dumps(changed_approvals(before, after), indent=2))


def replace(args):
    directory(args.backup)
    before, after = receipts(args.backup / "before"), receipts(args.backup / "after")
    if before.keys() != after.keys():
        raise ValueError("incomplete backup")
    rollback = args.operation == "rollback"
    if not rollback:
        approvals = {}
        for value in args.approve:
            name, separator, digest = value.partition("=")
            if not separator or name in approvals:
                raise ValueError("approvals must be distinct ID=DIGEST values")
            approvals[name] = digest
        if approvals != changed_approvals(before, after):
            raise ValueError("approve every changed selection with its exact digest")
        if regular(args.bindings_current) != regular(
            args.backup / "publishers.after.json"
        ):
            # Nix may format identical bindings differently.
            if json.loads(regular(args.bindings_current)) != json.loads(
                regular(args.backup / "publishers.after.json")
            ):
                raise ValueError(
                    "activate the reviewed bindings before applying receipts"
                )
    with locked(args.state):
        current = receipts(args.state)
        if current.keys() != before.keys() or any(
            current[name] not in (before[name], after[name]) for name in current
        ):
            raise ValueError("live selections changed; refusing migration or rollback")
        settled(args.roots, current)
        # All checks precede writes. A crash leaves only before/after bytes.
        # Repeating apply or rollback safely completes the chosen direction.
        for name, data in (before if rollback else after).items():
            atomic_write(args.state / name / "selection.json", data)
    print(
        "Restored original receipts"
        if rollback
        else "Applied reviewed receipts; run the normal host restore after releasing the lock"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    operations = parser.add_subparsers(dest="operation", required=True)
    for operation in ("prepare", "apply", "rollback"):
        sub = operations.add_parser(operation)
        sub.add_argument("--state", type=Path, required=True)
        sub.add_argument("--roots", type=Path, required=True)
        sub.add_argument("--backup", type=Path, required=True)
        if operation == "prepare":
            sub.add_argument("--host", type=Path, required=True)
            sub.add_argument("--nix", type=Path, default=Path("nix"))
            sub.add_argument("--bindings-before", type=Path, required=True)
            sub.add_argument("--bindings-after", type=Path, required=True)
        elif operation == "apply":
            sub.add_argument("--bindings-current", type=Path, required=True)
            sub.add_argument("--approve", action="append", default=[])
    args = parser.parse_args()
    locations = [path.resolve() for path in (args.state, args.roots, args.backup)]
    if any(
        a.is_relative_to(b)
        for index, a in enumerate(locations)
        for other, b in enumerate(locations)
        if index != other
    ):
        raise ValueError("state, roots and backup must be separate directory trees")
    prepare(args) if args.operation == "prepare" else replace(args)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error))
