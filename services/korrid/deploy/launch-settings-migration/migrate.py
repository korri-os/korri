#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.pyyaml
"""One-off ADR 0028 cut. Stop korrid, inspect --dry-run, then migrate once."""

import argparse
import copy
from datetime import datetime, timezone
import os
import math
import re
from pathlib import Path
import shutil
import tempfile

import yaml


class MigrationError(ValueError):
    pass


class UniqueLoader(yaml.SafeLoader):
    # Match serde_yaml's YAML 1.2 scalar interpretation. In particular, plugin
    # keys/values named yes, no, on or off must not become YAML 1.1 booleans.
    yaml_implicit_resolvers = copy.deepcopy(yaml.SafeLoader.yaml_implicit_resolvers)


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
    "tag:yaml.org,2002:int",
    re.compile(r"^[-+]?(?:0b[0-1_]+|0o[0-7_]+|0x[0-9a-fA-F_]+|[0-9][0-9_]*)$"),
    list("-+0123456789"),
)

UniqueLoader.add_implicit_resolver(
    "tag:yaml.org,2002:float",
    re.compile(
        r"^[-+]?(?:(?:[0-9][0-9_]*\.[0-9_]*|\.[0-9_]+)(?:[eE][-+]?[0-9]+)?|[0-9][0-9_]*[eE][-+]?[0-9]+|\.(?:inf|Inf|INF|nan|NaN|NAN))$"
    ),
    list("-+0123456789."),
)


def setting_scalar(value):
    return type(value) in {str, int, bool} or (
        type(value) is float and math.isfinite(value)
    )


def yaml_integer(loader, node):
    value = loader.construct_scalar(node).replace("_", "")
    sign = -1 if value.startswith("-") else 1
    unsigned = value.lstrip("+-")
    return sign * int(unsigned, 0 if unsigned.startswith(("0b", "0o", "0x")) else 10)


UniqueLoader.add_constructor("tag:yaml.org,2002:int", yaml_integer)


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


def record(value, path):
    if not isinstance(value, dict):
        raise MigrationError(f"{path} must be a record")
    return value


def keys(value, allowed, path):
    record(value, path)
    unknown = set(value) - set(allowed)
    if unknown:
        raise MigrationError(f"{path}: unknown keys: {sorted(unknown)}")


def rename(value, old, new, path):
    if old not in value:
        return
    if new in value:
        raise MigrationError(f"{path}: {old} clashes with {new}")
    value[new] = value.pop(old)


def hooks(value, path, routine=False):
    allowed = (
        {"before", "after"}
        if routine
        else {"before", "after", "use", "trust-removable"}
    )
    keys(value, allowed, path)
    if "trust-removable" in value:
        if not isinstance(value.pop("trust-removable"), bool):
            raise MigrationError(f"{path}.trust-removable must be boolean")
    if "use" in value and (
        not isinstance(value["use"], list)
        or not all(isinstance(v, str) for v in value["use"])
    ):
        raise MigrationError(f"{path}.use must be a name list")
    for phase in ("before", "after"):
        if phase not in value:
            continue
        if not isinstance(value[phase], list):
            raise MigrationError(f"{path}.{phase} must be a step list")
        for step in value[phase]:
            allowed = (
                {"run", "name", "timeout", "on-failure"}
                if phase == "before"
                else {"run", "name", "timeout"}
            )
            keys(step, allowed, f"{path}.{phase}")
            if not isinstance(step.get("run"), str) or not step["run"]:
                raise MigrationError(f"{path}.{phase}: step needs run")
            if "name" in step and (
                not isinstance(step["name"], str) or not step["name"]
            ):
                raise MigrationError(f"{path}.{phase}: step name must be nonempty")
            if "timeout" in step and (
                type(step["timeout"]) is not int or step["timeout"] <= 0
            ):
                raise MigrationError(f"{path}.{phase}: timeout must be positive")
            if "on-failure" in step and step["on-failure"] not in {"abort", "warn"}:
                raise MigrationError(f"{path}.{phase}: unknown on-failure policy")


PLACE = {
    "runner",
    "cpu",
    "gpu",
    "modifiers",
    "env",
    "cwd",
    "args",
    "argsAppend",
    "hooks",
    "families",
    "runners",
}
HOST = {"title", "preferences", "relays", "moonlight", "plugin", "patches"}
SYSTEM = {"name", "title", "manufacturer", "aliases", "metadata"}
GAME = {
    "title",
    "version-of",
    "relation",
    "contains",
    "display",
    "metadata",
    "releases",
    "profiles",
    "profile",
}
RELEASE = {"id", "system", "identity", "display"}


def place(value, path, extra=(), scoped=False):
    allowed = (
        (PLACE - {"runner", "families", "runners"}) | {"settings", "config"}
        if scoped
        else PLACE | set(extra)
    )
    keys(value, allowed, path)
    if "argsAppend" in value:
        args = value.setdefault("args", {})
        record(args, f"{path}.args")
        if "append" in args:
            raise MigrationError(f"{path}: argsAppend clashes with args.append")
        args["append"] = value.pop("argsAppend")
    for kind in ("cpu", "gpu"):
        if kind in value:
            keys(
                value[kind],
                {"min", "max", "governor"} if kind == "cpu" else {"min", "max"},
                f"{path}.{kind}",
            )
            for bound in ("min", "max"):
                if bound in value[kind]:
                    n = value[kind][bound]
                    if (
                        type(n) is not int
                        or not (0 if bound == "min" else 1) <= n <= 100
                    ):
                        raise MigrationError(
                            f"{path}.{kind}.{bound} is not a valid integer percent"
                        )
            if value[kind].get("min", 0) > value[kind].get("max", 100):
                raise MigrationError(f"{path}.{kind}: min exceeds max")
            if "governor" in value[kind] and (
                path != "host"
                or not isinstance(value[kind]["governor"], str)
                or not value[kind]["governor"]
            ):
                raise MigrationError(f"{path}.{kind}: governor belongs only to host")
    if "args" in value:
        keys(value["args"], {"prepend", "append"}, f"{path}.args")
        for v in value["args"].values():
            if not isinstance(v, list) or not all(isinstance(s, str) for s in v):
                raise MigrationError(f"{path}.args must contain string lists")
    if "env" in value:
        record(value["env"], f"{path}.env")
        if any(not isinstance(v, str) and v is not None for v in value["env"].values()):
            raise MigrationError(f"{path}.env values must be strings or null")
    if "cwd" in value and not isinstance(value["cwd"], str):
        raise MigrationError(f"{path}.cwd must be a string")
    if "hooks" in value:
        hooks(value["hooks"], f"{path}.hooks")
    if "runner" in value and (
        not isinstance(value["runner"], str) or not value["runner"]
    ):
        raise MigrationError(f"{path}.runner must be a nonempty string")
    if "config" in value:
        keys(value["config"], {"prepend", "append", "replace"}, f"{path}.config")
        if not all(isinstance(v, str) for v in value["config"].values()):
            raise MigrationError(f"{path}.config values must be strings")
    if "settings" in value:
        record(value["settings"], f"{path}.settings")  # Plugin-owned keys are opaque.
        if not all(setting_scalar(v) for v in value["settings"].values()):
            raise MigrationError(f"{path}.settings values must be scalars")
    for id, modifier in record(value.get("modifiers", {}), f"{path}.modifiers").items():
        keys(modifier, {"enabled", "settings", "config"}, f"{path}.modifiers.{id}")
        if "enabled" in modifier and type(modifier["enabled"]) is not bool:
            raise MigrationError(f"{path}.modifiers.{id}.enabled must be boolean")
        if "settings" in modifier:
            record(modifier["settings"], f"{path}.modifiers.{id}.settings")
            if not all(setting_scalar(v) for v in modifier["settings"].values()):
                raise MigrationError(
                    f"{path}.modifiers.{id}.settings values must be scalars"
                )
        if "config" in modifier:
            keys(modifier["config"], {"append"}, f"{path}.modifiers.{id}.config")
            if not all(isinstance(v, str) for v in modifier["config"].values()):
                raise MigrationError(
                    f"{path}.modifiers.{id}.config values must be strings"
                )
    if not scoped:
        for kind in ("families", "runners"):
            for id, child in record(value.get(kind, {}), f"{path}.{kind}").items():
                place(child, f"{path}.{kind}.{id}", scoped=True)
    if "preferences" in value:
        preferences = record(value["preferences"], f"{path}.preferences")
        rename(preferences, "playerCount", "player-count", path)
        keys(preferences, {"player-count", "launch"}, f"{path}.preferences")
    for id, profile in record(value.get("profiles", {}), f"{path}.profiles").items():
        place(profile, f"{path}.profiles.{id}", {"title"})
    if "releases" in value:
        if not isinstance(value["releases"], list):
            raise MigrationError(f"{path}.releases must be a list")
        for release in value["releases"]:
            place(release, f"{path}.releases", RELEASE)
            if "identity" in release:
                keys(
                    release["identity"], {"kind", "value", "parts"}, f"{path}.identity"
                )


def transform(device, library):
    device, library = copy.deepcopy(device), copy.deepcopy(library)
    keys(
        device,
        {
            "host",
            "storage",
            "providers",
            "systems",
            "runners",
            "families",
            "modifiers",
            "hooks",
            "routines",
            "locations",
            "sources",
            "provider-links",
        },
        "device",
    )
    keys(library, {"library"}, "library")
    for kind in ("runners", "families", "modifiers"):
        if kind in device:
            host = record(device.setdefault("host", {}), "host")
            if kind in host:
                raise MigrationError(f"device.{kind} clashes with host.{kind}")
            host[kind] = device.pop(kind)
    rename(device, "hooks", "routines", "device")
    if "host" in device:
        place(device["host"], "host", HOST)
    for id, system in record(device.get("systems", {}), "systems").items():
        place(system, f"systems.{id}", SYSTEM)
    for id, routine in record(device.get("routines", {}), "routines").items():
        hooks(routine, f"routines.{id}", routine=True)
    for id, game in record(library.get("library", {}), "library").items():
        place(game, f"library.{id}", GAME)
    for id, storage in record(device.get("storage", {}), "storage").items():
        keys(storage, {"root", "path"}, f"storage.{id}")
    for id, provider in record(device.get("providers", {}), "providers").items():
        keys(provider, {"title"}, f"providers.{id}")
    for id, source in record(device.get("sources", {}), "sources").items():
        keys(source, {"title", "kind", "storage", "system"}, f"sources.{id}")
    for id, link in record(device.get("provider-links", {}), "provider-links").items():
        keys(link, {"provider", "playable", "release", "refs"}, f"provider-links.{id}")
        if not isinstance(link.get("refs"), list):
            raise MigrationError(f"provider-links.{id}.refs must be a list")
        for ref in link["refs"]:
            rename(ref, "targetPart", "target-part", f"provider-links.{id}.refs")
            keys(
                ref,
                {"kind", "value", "scope", "target-part"},
                f"provider-links.{id}.refs",
            )
    for id, locations in record(device.get("locations", {}), "locations").items():
        if not isinstance(locations, list):
            raise MigrationError(f"locations.{id} must be a list")
        for location in locations:
            keys(
                location,
                {
                    "storage",
                    "path",
                    "root",
                    "files",
                    "discovery",
                    "value",
                    "provider",
                    "ref",
                },
                f"locations.{id}",
            )
            if "discovery" in location:
                keys(
                    location["discovery"],
                    {"first-seen-at", "missing-since"},
                    f"locations.{id}.discovery",
                )
            if "files" in location:
                if not isinstance(location["files"], list):
                    raise MigrationError(f"locations.{id}.files must be a list")
                for file in location["files"]:
                    keys(file, {"id", "role", "path"}, f"locations.{id}.files")
    return device, library


def korrid_running(proc_root=Path("/proc")):
    for entry in proc_root.iterdir():
        if entry.name.isdigit():
            try:
                if (entry / "comm").read_text().strip() == "korrid":
                    return True
            except (FileNotFoundError, PermissionError, ProcessLookupError):
                pass
    return False


def migrate(root, dry_run=False):
    if korrid_running():
        raise MigrationError("stop korrid before migrating")
    paths = [root / "device.yaml", root / "catalog/library.yaml"]
    for path in paths:
        if path.is_symlink() or not path.is_file():
            raise MigrationError(f"expected a regular file: {path}")
    documents = [yaml.load(path.read_text(), Loader=UniqueLoader) for path in paths]
    if any(document is None for document in documents):
        raise MigrationError("empty documents are not a known configuration shape")
    migrated = transform(*documents)
    changed = tuple(documents) != migrated
    if dry_run or not changed:
        return changed, None
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    backup = root / f"launch-settings-backup-{stamp}"
    backup.mkdir(mode=0o700)
    (backup / "catalog").mkdir()
    for path in paths:
        shutil.copy2(path, backup / path.relative_to(root))
    temporary = []
    try:
        for path, document in zip(paths, migrated):
            fd, name = tempfile.mkstemp(prefix=".launch-settings-", dir=path.parent)
            temporary.append(Path(name))
            with os.fdopen(fd, "w") as output:
                os.fchmod(output.fileno(), path.stat().st_mode & 0o777)
                yaml.safe_dump(document, output, sort_keys=False, allow_unicode=True)
                output.flush()
                os.fsync(output.fileno())
        for source, target in zip(temporary, paths):
            os.replace(source, target)
        for directory in {path.parent for path in paths}:
            fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(fd)
            finally:
                os.close(fd)
    finally:
        for path in temporary:
            path.unlink(missing_ok=True)
    return True, backup


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--storage-root", type=Path, required=True)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    try:
        changed, backup = migrate(args.storage_root, args.dry_run)
    except (MigrationError, OSError, yaml.YAMLError) as error:
        parser.exit(1, f"migration refused: {error}\n")
    print(
        "changes required"
        if args.dry_run and changed
        else "migrated"
        if changed
        else "already migrated"
    )
    if backup:
        print(f"backup: {backup}")


if __name__ == "__main__":
    main()
