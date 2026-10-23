#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.pyyaml
"""One-off ADR 0028 cut. Stop korrid, inspect --dry-run, then migrate once."""

import argparse
import copy
from datetime import datetime, timezone
from dataclasses import dataclass
import os
import math
import hashlib
import re
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile

import yaml


class MigrationError(ValueError):
    pass


# config/place.rs decodes through serde_yaml::Value before typed fields. Its
# integer visitors are i64/u64; LaunchSettingValue's serde_json::Number has the
# same limits. Typed CPU, player-count and timeout checks remain narrower.
YAML_INTEGER_MIN = -(2**63)
YAML_INTEGER_MAX = 2**64 - 1
INTEGER_LITERAL = re.compile(
    r"^[-+]?(?:0b[01]+|0o[0-7]+|0x[0-9a-fA-F]+|0|[1-9][0-9]*)$"
)


class UniqueLoader(yaml.SafeLoader):
    # Match serde_yaml's YAML 1.2 scalar interpretation. In particular, plugin
    # keys/values named yes, no, on or off must not become YAML 1.1 booleans.
    yaml_implicit_resolvers = copy.deepcopy(yaml.SafeLoader.yaml_implicit_resolvers)

    def resolve(self, kind, value, implicit):
        tag = super().resolve(kind, value, implicit)
        if kind is yaml.nodes.ScalarNode and tag == "tag:yaml.org,2002:float":
            special = {".inf", "+.inf", "-.inf", ".nan"}
            # serde_yaml::parse_f64 leaves an overflowing untagged exponent as
            # a string; do not turn it into infinity or change its meaning.
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
    "tag:yaml.org,2002:int",
    INTEGER_LITERAL,
    list("-+0123456789"),
)

UniqueLoader.add_implicit_resolver(
    "tag:yaml.org,2002:float",
    re.compile(
        r"^(?:[-+]?(?:(?:[0-9]+\.[0-9]*|\.[0-9]+)(?:[eE][-+]?[0-9]+)?|[0-9]+[eE][-+]?[0-9]+|\.(?:inf|Inf|INF))|\.(?:nan|NaN|NAN))$"
    ),
    list("-+0123456789."),
)


def setting_scalar(value):
    if type(value) in {str, bool}:
        return True
    if type(value) is int:
        return YAML_INTEGER_MIN <= value <= YAML_INTEGER_MAX
    return type(value) is float and math.isfinite(value)


def yaml_integer(loader, node):
    value = loader.construct_scalar(node)
    if not INTEGER_LITERAL.fullmatch(value):
        invalid(
            f"YAML integer at line {node.start_mark.line + 1}",
            value,
            "a serde_yaml integer literal",
        )
    sign = -1 if value.startswith("-") else 1
    unsigned = value.lstrip("+-")
    try:
        return sign * int(
            unsigned, 0 if unsigned.startswith(("0b", "0o", "0x")) else 10
        )
    except ValueError as error:
        raise MigrationError(
            f"YAML integer at line {node.start_mark.line + 1}: value {value!r}; invalid integer"
        ) from error


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


def invalid(path, value, expected):
    raise MigrationError(f"{path}: value {value!r}; expected {expected}")


def record(value, path):
    if not isinstance(value, dict):
        invalid(path, value, "a record")
    return value


def keys(value, allowed, path):
    record(value, path)
    for key in value:
        if key not in allowed:
            invalid(f"{path}.{key}", value[key], "a recognized key")


def text(value, path, *, nonempty=False, no_nul=False):
    if (
        not isinstance(value, str)
        or (nonempty and not value)
        or (no_nul and "\0" in value)
    ):
        invalid(
            path,
            value,
            "a nonempty string"
            if nonempty
            else "a string without NUL"
            if no_nul
            else "a string",
        )


def integer(value, path, lower, upper):
    if type(value) is not int or not lower <= value <= upper:
        invalid(path, value, f"an integer from {lower} to {upper}")


def validate_integer_scalars(value, path):
    if type(value) is int:
        integer(value, path, YAML_INTEGER_MIN, YAML_INTEGER_MAX)
    elif isinstance(value, dict):
        for key, entry in value.items():
            validate_integer_scalars(entry, f"{path}.{key}")
    elif isinstance(value, list):
        for index, entry in enumerate(value):
            validate_integer_scalars(entry, f"{path}[{index}]")


def boolean(value, path):
    if type(value) is not bool:
        invalid(path, value, "a boolean")


def sequence(value, path):
    if not isinstance(value, list):
        invalid(path, value, "a list")
    return value


def texts(value, path, *, nonempty=False, no_nul=False):
    for index, entry in enumerate(sequence(value, path)):
        text(entry, f"{path}[{index}]", nonempty=nonempty, no_nul=no_nul)


def scalar_settings(value, path):
    for key, entry in record(value, path).items():
        if not setting_scalar(entry):
            invalid(f"{path}.{key}", entry, "a boolean, finite number or string")


def json_value(value, path):
    if value is None or setting_scalar(value):
        return
    if isinstance(value, dict):
        for key, entry in value.items():
            text(key, path)
            json_value(entry, f"{path}.{key}")
    elif isinstance(value, list):
        for index, entry in enumerate(value):
            json_value(entry, f"{path}[{index}]")
    else:
        invalid(path, value, "a JSON value")


def native_config(value, path, allowed, *, nullable=False):
    if nullable and value is None:
        return
    keys(value, allowed, path)
    for key, entry in value.items():
        if not nullable or entry is not None:
            text(entry, f"{path}.{key}")


def provider_id(value, path):
    if not isinstance(value, str) or not re.fullmatch(
        r"@[a-z0-9][a-z0-9._-]*:[a-z0-9][a-z0-9._-]*", value
    ):
        invalid(path, value, "a plugin ID such as @korri:example")


def scope_id(value, path, family=False, modifier=False):
    if family:
        provider_id(value, path)
        return
    text(value, path, nonempty=True)
    owner, slash, reference = value.partition("/")
    provider_id(owner, path)
    if (
        not slash
        or not reference
        or len(reference.encode("utf-8")) > 2048
        or any(ord(c) < 32 or ord(c) == 127 for c in reference)
    ):
        invalid(path, value, "a fully qualified contribution ID")
    if modifier and not re.fullmatch(r"[a-z0-9][a-z0-9._-]*", reference):
        invalid(path, value, "a modifier contribution ID")


def rename(value, old, new, path):
    if old not in value:
        return
    if new in value:
        invalid(
            f"{path}.{old}", value[old], f"no clash with {path}.{new}={value[new]!r}"
        )
    value[new] = value.pop(old)


def modifiers(value, path):
    for id, modifier in record(value, path).items():
        current = f"{path}.{id}"
        scope_id(id, current, modifier=True)
        keys(modifier, {"enabled", "settings", "config"}, current)
        if "enabled" in modifier:
            boolean(modifier["enabled"], f"{current}.enabled")
        if "settings" in modifier:
            scalar_settings(modifier["settings"], f"{current}.settings")
        if "config" in modifier:
            native_config(modifier["config"], f"{current}.config", {"append"})


def hooks(value, path, routine_names, routine=False):
    allowed = (
        {"before", "after"}
        if routine
        else {"before", "after", "use", "trust-removable"}
    )
    keys(value, allowed, path)
    if "trust-removable" in value:
        boolean(value["trust-removable"], f"{path}.trust-removable")
        value.pop("trust-removable")
    if "use" in value:
        texts(value["use"], f"{path}.use", nonempty=True)
        for index, name in enumerate(value["use"]):
            if name not in routine_names:
                invalid(f"{path}.use[{index}]", name, "an existing routine name")
    for phase in ("before", "after"):
        if phase not in value:
            continue
        for index, step in enumerate(sequence(value[phase], f"{path}.{phase}")):
            current = f"{path}.{phase}[{index}]"
            allowed = (
                {"run", "name", "timeout", "on-failure"}
                if phase == "before"
                else {"run", "name", "timeout"}
            )
            keys(step, allowed, current)
            text(step.get("run"), f"{current}.run", nonempty=True)
            if "name" in step:
                text(step["name"], f"{current}.name", nonempty=True)
            if "timeout" in step:
                integer(step["timeout"], f"{current}.timeout", 1, YAML_INTEGER_MAX)
            if "on-failure" in step and (
                not isinstance(step["on-failure"], str)
                or step["on-failure"] not in {"abort", "warn"}
            ):
                invalid(f"{current}.on-failure", step["on-failure"], "abort or warn")


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


def place(value, path, extra=(), scoped=False, routine_names=()):
    allowed = (
        (PLACE - {"runner", "families", "runners"}) | {"settings", "config"}
        if scoped
        else PLACE | set(extra)
    )
    keys(value, allowed, path)
    if "argsAppend" in value:
        texts(value["argsAppend"], f"{path}.argsAppend", no_nul=True)
        args = value.setdefault("args", {})
        record(args, f"{path}.args")
        if "append" in args:
            invalid(
                f"{path}.argsAppend",
                value["argsAppend"],
                f"no clash with {path}.args.append={args['append']!r}",
            )
        args["append"] = value.pop("argsAppend")
    for kind in ("cpu", "gpu"):
        if kind not in value:
            continue
        current = f"{path}.{kind}"
        bounds = value[kind]
        keys(
            bounds,
            {"min", "max", "governor"} if kind == "cpu" else {"min", "max"},
            current,
        )
        for bound in ("min", "max"):
            if bound in bounds:
                integer(
                    bounds[bound], f"{current}.{bound}", 0 if bound == "min" else 1, 100
                )
        if bounds.get("min", 0) > bounds.get("max", 100):
            invalid(current, bounds, "min <= max")
        if "governor" in bounds:
            text(bounds["governor"], f"{current}.governor", nonempty=True)
            if path != "host":
                invalid(
                    f"{current}.governor", bounds["governor"], "a governor only on host"
                )
    if "args" in value:
        keys(value["args"], {"prepend", "append"}, f"{path}.args")
        for kind, args in value["args"].items():
            texts(args, f"{path}.args.{kind}", no_nul=True)
    if "env" in value:
        for key, entry in record(value["env"], f"{path}.env").items():
            if not key or "=" in key or "\0" in key:
                invalid(
                    f"{path}.env", key, "a nonempty environment name without '=' or NUL"
                )
            if entry is not None:
                text(entry, f"{path}.env.{key}", no_nul=True)
    if "cwd" in value:
        text(value["cwd"], f"{path}.cwd", no_nul=True)
    if "hooks" in value:
        hooks(value["hooks"], f"{path}.hooks", routine_names)
    if "runner" in value:
        text(value["runner"], f"{path}.runner", nonempty=True)
    if "config" in value:
        native_config(
            value["config"],
            f"{path}.config",
            {"prepend", "append", "replace"},
            nullable=True,
        )
    if "settings" in value:
        scalar_settings(value["settings"], f"{path}.settings")
    if "modifiers" in value:
        modifiers(value["modifiers"], f"{path}.modifiers")
    if not scoped:
        for kind in ("families", "runners"):
            for id, child in record(value.get(kind, {}), f"{path}.{kind}").items():
                current = f"{path}.{kind}.{id}"
                scope_id(id, current, family=kind == "families")
                place(child, current, scoped=True, routine_names=routine_names)
    if "preferences" in value:
        preferences = record(value["preferences"], f"{path}.preferences")
        keys(
            preferences,
            {"playerCount", "player-count", "launch"},
            f"{path}.preferences",
        )
        for key in ("playerCount", "player-count"):
            if key in preferences:
                integer(preferences[key], f"{path}.preferences.{key}", 1, 255)
        rename(preferences, "playerCount", "player-count", f"{path}.preferences")
        if "launch" in preferences:
            invalid(
                f"{path}.preferences.launch",
                preferences["launch"],
                "host launch preferences to remain absent (unsupported by korrid)",
            )
    for key in ("title", "name", "manufacturer"):
        if key in value:
            text(
                value[key],
                f"{path}.{key}",
                nonempty=key == "title" and set(extra) == {"title"},
            )
    for key in ("system", "profile", "version-of", "relation"):
        if key in value:
            text(value[key], f"{path}.{key}", nonempty=True)
    for key in ("relays", "aliases"):
        if key in value:
            texts(value[key], f"{path}.{key}")
    if "plugin" in value:
        for id, enabled in record(value["plugin"], f"{path}.plugin").items():
            provider_id(id, f"{path}.plugin.{id}")
            boolean(enabled, f"{path}.plugin.{id}")
    for key in ("moonlight", "patches"):
        if key in value:
            invalid(
                f"{path}.{key}", value[key], "unsupported host policy to remain absent"
            )
    for key in ("metadata", "contains", "display"):
        if key in value:
            record(value[key], f"{path}.{key}")
            json_value(value[key], f"{path}.{key}")
    for id, profile in record(value.get("profiles", {}), f"{path}.profiles").items():
        text(id, f"{path}.profiles", nonempty=True)
        text(
            record(profile, f"{path}.profiles.{id}").get("title"),
            f"{path}.profiles.{id}.title",
            nonempty=True,
        )
        place(profile, f"{path}.profiles.{id}", {"title"}, routine_names=routine_names)
    if "profile" in value and value["profile"] not in value.get("profiles", {}):
        invalid(f"{path}.profile", value["profile"], "an existing profile ID")
    if "releases" in value:
        for index, release in enumerate(
            sequence(value["releases"], f"{path}.releases")
        ):
            current = f"{path}.releases[{index}]"
            place(release, current, RELEASE, routine_names=routine_names)
            text(release.get("id"), f"{current}.id", nonempty=True)
            if "identity" in release:
                keys(
                    release["identity"],
                    {"kind", "value", "parts"},
                    f"{current}.identity",
                )


def release_key(value, path):
    text(value, path, nonempty=True)
    if re.fullmatch(r"sha256:[0-9a-f]{64}|sha1:[0-9a-f]{40}", value):
        return
    scope_id(value, path)


def identity(value, path):
    keys(value, {"kind", "value", "parts"}, path)
    kind = value.get("kind")
    if not isinstance(kind, str) or kind not in {"hash", "chd", "parts", "provider"}:
        invalid(f"{path}.kind", kind, "hash, chd, parts or provider")
    release_key(value.get("value"), f"{path}.value")
    content = value["value"]
    if kind == "hash" and not content.startswith(("sha256:", "sha1:")):
        invalid(f"{path}.value", content, "a SHA-256 or SHA-1 hash identity")
    if kind == "chd" and not content.startswith("sha1:"):
        invalid(f"{path}.value", content, "a SHA-1 CHD identity")
    if kind == "provider" and not content.startswith("@"):
        invalid(f"{path}.value", content, "a provider release identity")
    if kind != "parts" and "parts" in value:
        invalid(f"{path}.parts", value["parts"], "parts only on a parts identity")
    if kind == "parts":
        parts = sequence(value.get("parts"), f"{path}.parts")
        if not parts:
            invalid(f"{path}.parts", parts, "a nonempty list of release identities")
        for index, part in enumerate(parts):
            release_key(part, f"{path}.parts[{index}]")
        expected = "sha256:" + hashlib.sha256("\n".join(parts).encode()).hexdigest()
        if content != expected:
            invalid(f"{path}.value", content, f"the ordered parts hash {expected}")


def target(value, path):
    text(value, path, nonempty=True)
    if value.startswith("/"):
        invalid(path, value, "a non-absolute target path")


def discovery(value, path):
    keys(value, {"first-seen-at", "missing-since"}, path)
    for key in ("first-seen-at", "missing-since"):
        if key not in value and key != "first-seen-at":
            continue
        stamp = value.get(key)
        text(stamp, f"{path}.{key}", nonempty=True)
        if not re.fullmatch(
            r"[0-9]{4}-[0-9]{2}-[0-9]{2}[Tt][0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]+)?(?:[Zz]|[+-][0-9]{2}:[0-9]{2})",
            stamp,
        ):
            invalid(f"{path}.{key}", stamp, "an RFC 3339 timestamp")
        try:
            # chrono permits leap seconds. Validate their calendar with :59.
            normalized = (
                stamp[:17]
                + ("59" if stamp[17:19] == "60" else stamp[17:19])
                + stamp[19:]
            )
            datetime.fromisoformat(normalized.upper().replace("Z", "+00:00"))
        except ValueError:
            invalid(f"{path}.{key}", stamp, "an RFC 3339 timestamp")


def location(value, path, release):
    record(value, path)
    if "storage" in value:
        allowed = (
            {"storage", "path", "discovery"}
            if "files" not in value
            else {"storage", "root", "files", "discovery"}
        )
        keys(value, allowed, path)
        text(value.get("storage"), f"{path}.storage", nonempty=True)
        if "files" not in value:
            target(value.get("path"), f"{path}.path")
        else:
            if "root" in value:
                target(value["root"], f"{path}.root")
            parts = sequence(value["files"], f"{path}.files")
            if not parts:
                invalid(f"{path}.files", parts, "a nonempty file list")
            ids = set()
            for index, part in enumerate(parts):
                current = f"{path}.files[{index}]"
                keys(part, {"id", "role", "path"}, current)
                text(part.get("id"), f"{current}.id", nonempty=True)
                if part["id"] in ids:
                    invalid(f"{current}.id", part["id"], "a unique file ID")
                ids.add(part["id"])
                if "role" in part:
                    text(part["role"], f"{current}.role", nonempty=True)
                target(part.get("path"), f"{current}.path")
        if "discovery" in value:
            discovery(value["discovery"], f"{path}.discovery")
    elif "path" in value:
        keys(value, {"path"}, path)
        target(value["path"], f"{path}.path")
    elif "value" in value:
        keys(value, {"value"}, path)
        target(value["value"], f"{path}.value")
    else:
        keys(value, {"provider", "ref"}, path)
        provider_id(value.get("provider"), f"{path}.provider")
        text(value.get("ref"), f"{path}.ref", nonempty=True)
        if release != f"{value['provider']}/{value['ref']}":
            invalid(path, value, f"a provider location matching release {release}")


def validate_catalog(device, library):
    # Mirrors config/catalog.rs for the records a migration preserves beside
    # moved values. The optional Rust guard independently checks this boundary.
    identities = set()
    for game_id, game in library.get("library", {}).items():
        path = f"library.{game_id}"
        if not re.fullmatch(r"[a-z0-9][a-z0-9._-]*", game_id):
            invalid(path, game_id, "a readable local game ID")
        releases = sequence(game.get("releases"), f"{path}.releases")
        if not releases:
            invalid(f"{path}.releases", releases, "at least one release")
        ids = set()
        for index, release in enumerate(releases):
            current = f"{path}.releases[{index}]"
            id = release["id"]
            if not re.fullmatch(r"[a-z0-9][a-z0-9._-]*", id) or id in ids:
                invalid(f"{current}.id", id, "a unique readable local release ID")
            ids.add(id)
            if "identity" in release:
                identity(release["identity"], f"{current}.identity")
                key = release["identity"]["value"]
                if key in identities:
                    invalid(
                        f"{current}.identity.value", key, "a unique content identity"
                    )
                identities.add(key)
                if release["identity"]["kind"] == "provider" and any(
                    "provider" not in loc
                    for loc in device.get("locations", {}).get(key, [])
                ):
                    invalid(f"{current}.identity.value", key, "provider locations only")
    for id, source in device.get("sources", {}).items():
        if not re.fullmatch(r"[a-z0-9][a-z0-9._-]*", id):
            invalid(f"sources.{id}", id, "a readable local source ID")
        if source["storage"] not in device.get("storage", {}):
            invalid(
                f"sources.{id}.storage", source["storage"], "an existing storage ID"
            )
    for id, link in device.get("provider-links", {}).items():
        if link["playable"] not in library.get("library", {}):
            invalid(
                f"provider-links.{id}.playable", link["playable"], "an existing game ID"
            )


def transform(device, library):
    device, library = copy.deepcopy(device), copy.deepcopy(library)
    # Check every scalar, including opaque JSON metadata and lists, before any
    # move, rename, external guard, backup or write can consume the documents.
    validate_integer_scalars(device, "device.yaml")
    validate_integer_scalars(library, "catalog/library.yaml")
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
    routine_names = record(
        device.get("routines", device.get("hooks", {})),
        "routines" if "routines" in device else "hooks",
    )
    for kind in ("runners", "families"):
        for id, child in record(device.get(kind, {}), kind).items():
            current = f"{kind}.{id}"
            scope_id(id, current, family=kind == "families")
            place(child, current, scoped=True, routine_names=routine_names)
    if "modifiers" in device:
        modifiers(device["modifiers"], "modifiers")
    if "hooks" in device:
        for id, routine in record(device["hooks"], "hooks").items():
            text(id, "hooks", nonempty=True)
            hooks(routine, f"hooks.{id}", routine_names, routine=True)
    for kind in ("runners", "families", "modifiers"):
        if kind in device:
            host = record(device.setdefault("host", {}), "host")
            if kind in host:
                raise MigrationError(f"device.{kind} clashes with host.{kind}")
            host[kind] = device.pop(kind)
    rename(device, "hooks", "routines", "device")
    if "host" in device:
        place(device["host"], "host", HOST, routine_names=routine_names)
    for id, system in record(device.get("systems", {}), "systems").items():
        text(id, "systems", nonempty=True)
        place(system, f"systems.{id}", SYSTEM, routine_names=routine_names)
    for id, routine in record(device.get("routines", {}), "routines").items():
        text(id, "routines", nonempty=True)
        hooks(routine, f"routines.{id}", routine_names, routine=True)
    for id, game in record(library.get("library", {}), "library").items():
        place(game, f"library.{id}", GAME, routine_names=routine_names)
    for id, storage in record(device.get("storage", {}), "storage").items():
        text(id, "storage", nonempty=True)
        keys(storage, {"root", "path"}, f"storage.{id}")
        text(storage.get("root"), f"storage.{id}.root", nonempty=True)
        if "path" in storage:
            for key, entry in record(storage["path"], f"storage.{id}.path").items():
                text(entry, f"storage.{id}.path.{key}", nonempty=True)
    for id, provider in record(device.get("providers", {}), "providers").items():
        provider_id(id, f"providers.{id}")
        keys(provider, {"title"}, f"providers.{id}")
        if "title" in provider:
            text(provider["title"], f"providers.{id}.title")
    for id, source in record(device.get("sources", {}), "sources").items():
        keys(source, {"title", "kind", "storage", "system"}, f"sources.{id}")
        for key in ("storage", "system"):
            text(source.get(key), f"sources.{id}.{key}", nonempty=True)
        texts(source.get("kind"), f"sources.{id}.kind", nonempty=True)
        if "title" in source:
            text(source["title"], f"sources.{id}.title")
    for id, link in record(device.get("provider-links", {}), "provider-links").items():
        keys(link, {"provider", "playable", "release", "refs"}, f"provider-links.{id}")
        provider_id(link.get("provider"), f"provider-links.{id}.provider")
        text(link.get("playable"), f"provider-links.{id}.playable", nonempty=True)
        if "release" in link:
            text(link["release"], f"provider-links.{id}.release", nonempty=True)
        refs = sequence(link.get("refs"), f"provider-links.{id}.refs")
        if not refs:
            invalid(f"provider-links.{id}.refs", refs, "a nonempty reference list")
        for index, ref in enumerate(refs):
            current = f"provider-links.{id}.refs[{index}]"
            keys(ref, {"kind", "value", "scope", "targetPart", "target-part"}, current)
            for key in ("targetPart", "target-part", "scope"):
                if key in ref:
                    text(ref[key], f"{current}.{key}", nonempty=True)
            rename(ref, "targetPart", "target-part", current)
            text(ref.get("value"), f"{current}.value", nonempty=True)
            if not isinstance(ref.get("kind"), str) or ref["kind"] not in {
                "url",
                "provider-item-id",
                "external-id",
            }:
                invalid(
                    f"{current}.kind",
                    ref.get("kind"),
                    "url, provider-item-id or external-id",
                )
    for id, locations in record(device.get("locations", {}), "locations").items():
        release_key(id, f"locations.{id}")
        for index, entry in enumerate(sequence(locations, f"locations.{id}")):
            location(entry, f"locations.{id}[{index}]", id)
    validate_catalog(device, library)
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


def check_with_korrid(documents, probe):
    # The probe is prebuilt and optional. Never compile on the device. Captured
    # candidate files live outside the storage root and are deleted on refusal.
    with tempfile.TemporaryDirectory(
        prefix="korri-launch-settings-check-"
    ) as directory:
        root = Path(directory)
        paths = [root / "device.yaml", root / "library.yaml"]
        for path, document in zip(paths, documents):
            path.write_text(
                yaml.safe_dump(document, sort_keys=False, allow_unicode=True)
            )
        try:
            result = subprocess.run(
                [str(probe), "--check", *(str(p) for p in paths)],
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise MigrationError(f"config-check {probe!s} failed: {error}") from error
        if result.returncode:
            diagnostic = (result.stderr or result.stdout).strip() or "no diagnostic"
            raise MigrationError(
                f"config-check refused transformed documents (probe={probe}, status={result.returncode}): {diagnostic}"
            )


POSIX_ACL_ACCESS = "system.posix_acl_access"


@dataclass(frozen=True)
class FileMetadata:
    uid: int
    gid: int
    mode: int
    xattrs: dict[str, bytes]


def metadata_from_fd(fd: int) -> FileMetadata:
    status = os.fstat(fd)
    return FileMetadata(
        status.st_uid,
        status.st_gid,
        stat.S_IMODE(status.st_mode),
        {name: os.getxattr(fd, name) for name in os.listxattr(fd)},
    )


def read_file_metadata(path: Path) -> FileMetadata:
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
        try:
            if not stat.S_ISREG(os.fstat(fd).st_mode):
                raise MigrationError(f"metadata source is not a regular file: {path}")
            return metadata_from_fd(fd)
        finally:
            os.close(fd)
    except OSError as error:
        raise MigrationError(f"cannot read metadata for {path}: {error}") from error


def apply_file_metadata(
    fd: int, expected: FileMetadata, source: Path, destination: Path
) -> None:
    try:
        current = os.fstat(fd)
        if (current.st_uid, current.st_gid) != (expected.uid, expected.gid):
            os.fchown(fd, expected.uid, expected.gid)
        # These files are unpublished. Keep them private and writable while
        # installing user xattrs, even for a 0400 source. chown/write can clear
        # set-ID bits, so restore the complete mode after writing the content.
        os.fchmod(fd, 0o600)
        current_xattrs = {name: os.getxattr(fd, name) for name in os.listxattr(fd)}
        for name in current_xattrs:
            if name == POSIX_ACL_ACCESS or name not in expected.xattrs:
                os.removexattr(fd, name)
        for name, value in expected.xattrs.items():
            if name != POSIX_ACL_ACCESS and current_xattrs.get(name) != value:
                os.setxattr(fd, name, value)
        os.fchmod(fd, expected.mode)
        # chmod changes the ACL mask. Install the original ACL last; never
        # retain a default ACL inherited from a destination directory.
        if POSIX_ACL_ACCESS in expected.xattrs:
            os.setxattr(fd, POSIX_ACL_ACCESS, expected.xattrs[POSIX_ACL_ACCESS])
        if metadata_from_fd(fd) != expected:
            raise MigrationError(
                f"metadata verification failed for {source} -> {destination}"
            )
    except OSError as error:
        raise MigrationError(
            f"cannot preserve metadata for {source} -> {destination}: {error}"
        ) from error


def backup_file(source: Path, destination: Path, metadata: FileMetadata) -> None:
    # Open before copy2 applies a potentially read-only mode. copy2's xattr
    # copy is best-effort; the explicit copy below must succeed and verify.
    fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        shutil.copy2(source, destination)
        apply_file_metadata(fd, metadata, source, destination)
        os.fsync(fd)
    finally:
        os.close(fd)


def fsync_directory(path: Path) -> None:
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def migrate(root, dry_run=False, config_check=None):
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
    if config_check is not None:
        check_with_korrid(migrated, config_check)
    changed = tuple(documents) != migrated
    if dry_run or not changed:
        return changed, None
    metadata = [read_file_metadata(path) for path in paths]
    temporary = []
    backup = None
    backup_created = False
    publish_started = False
    try:
        # Preflight both files' metadata before writing candidate YAML. Do not
        # replace either original until both candidates and complete backups
        # have been fsynced with their original ownership, mode and xattrs.
        for path, expected in zip(paths, metadata):
            fd, name = tempfile.mkstemp(prefix=".launch-settings-", dir=path.parent)
            destination = Path(name)
            temporary.append((fd, destination))
            apply_file_metadata(fd, expected, path, destination)
        for path, document, expected, (fd, destination) in zip(
            paths, migrated, metadata, temporary
        ):
            with os.fdopen(fd, "w", closefd=False) as output:
                yaml.safe_dump(document, output, sort_keys=False, allow_unicode=True)
                output.flush()
                apply_file_metadata(fd, expected, path, destination)
                os.fsync(fd)
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
        backup = root / f"launch-settings-backup-{stamp}"
        backup.mkdir(mode=0o700)
        backup_created = True
        (backup / "catalog").mkdir(mode=0o700)
        for path, expected in zip(paths, metadata):
            backup_file(path, backup / path.relative_to(root), expected)
        for directory in [backup / "catalog", backup, root]:
            fsync_directory(directory)
        publish_started = True
        for (_, source), target in zip(temporary, paths):
            os.replace(source, target)
        for directory in {path.parent for path in paths}:
            fsync_directory(directory)
    except BaseException:
        if backup_created and not publish_started:
            shutil.rmtree(backup)
        raise
    finally:
        for fd, path in temporary:
            os.close(fd)
            path.unlink(missing_ok=True)
    return True, backup


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--storage-root", type=Path, required=True)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument(
        "--config-check",
        type=Path,
        help="optional prebuilt config_snapshot_probe for a second read-only check",
    )
    args = parser.parse_args()
    try:
        changed, backup = migrate(args.storage_root, args.dry_run, args.config_check)
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
