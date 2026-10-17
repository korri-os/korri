#!/usr/bin/env python3
"""Merge native Steam settings while the client is stopped.

Only edited values and new keys change. Unknown keys, comments, order and
platform conditionals stay byte-for-byte intact. Malformed input fails closed.
Paths and seeds come from legacy state-materializer.ts and steam-gate-seed.ts.
"""
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile

TOOL = "proton-cachyos-11.0-20260601-slr-arm64"
ONCE = (
    "Intro", "NonVerifiedGame", "Gyro", "RemotePlayConfirm",
    "ExternalControllersAndSIAPI", "IntroToActionSets", "IntroToSteamInputGames",
)
PER_GAME = (
    "AppHasSmallText", "AppTextInputDoesNotAutomaticallyInvokesKeyboard",
    "AppLauncherInteractionIssues", "GamepadRecommended", "CurrentGamepadUnsupported",
    "CurrentGamepadSteamInputOptIn", "IntroToVRTheater", "HDRRequiresUserAction",
)
TOKEN = re.compile(r'\s+|//[^\r\n]*|"(?:\\.|[^"\\])*"|[{}]|[^\s{}"]+')


@dataclass
class Token:
    value: str
    start: int
    end: int
    kind: str


@dataclass
class Object:
    entries: list
    end: int


def parse(text: str) -> Object:
    tokens = []
    offset = 0
    for match in TOKEN.finditer(text):
        if match.start() != offset:
            raise ValueError("invalid VDF token")
        offset = match.end()
        raw = match.group()
        if raw.isspace() or raw.startswith("//"):
            continue
        # Valve accepts backslash-escaped quotes and backslashes. Leave other
        # escape sequences untouched rather than applying JSON's wider grammar.
        value = re.sub(r'\\(["\\])', r'\1', raw[1:-1]) if raw.startswith('"') else raw
        kind = "brace" if raw in ("{", "}") else "string" if raw.startswith('"') else "word"
        tokens.append(Token(value, match.start(), match.end(), kind))
    if offset != len(text):
        raise ValueError("unterminated VDF string")
    position = 0

    def body(nested=False):
        nonlocal position
        entries = []
        while position < len(tokens):
            key = tokens[position]
            position += 1
            if key.kind == "brace" and key.value == "}":
                if not nested:
                    raise ValueError("unexpected closing brace")
                return Object(entries, key.start)
            if key.kind == "brace" or position == len(tokens):
                raise ValueError("missing VDF key or value")
            value = tokens[position]
            position += 1
            if value.kind == "brace" and value.value == "{":
                value = body(True)
            elif value.kind == "brace":
                raise ValueError("missing VDF value")
            entries.append((key, value))
            if (position < len(tokens) and tokens[position].kind == "word"
                    and tokens[position].value.startswith("[")):
                position += 1  # Preserve native [$LINUX] conditionals.
        if nested:
            raise ValueError("unclosed VDF object")
        return Object(entries, len(text))

    return body()


def quote(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def render(values: dict, depth=0) -> str:
    lines = []
    pad = "\t" * depth
    for key, value in values.items():
        if isinstance(value, dict):
            lines.extend([f"{pad}{quote(key)}\n{pad}{{\n", render(value, depth + 1), f"{pad}}}\n"])
        else:
            lines.append(f"{pad}{quote(key)}\t\t{quote(value)}\n")
    return "".join(lines)


def merge(text: str, values: dict) -> str:
    root = parse(text)
    edits = []

    def visit(node, desired, depth=0):
        missing = {}
        for key, value in desired.items():
            found = [(k, v) for k, v in node.entries if k.value.lower() == key.lower()]
            if len(found) > 1:
                raise ValueError(f"ambiguous duplicate VDF key: {key}")
            if not found:
                missing[key] = value
                continue
            _, current = found[0]
            if isinstance(value, dict):
                if not isinstance(current, Object):
                    raise ValueError(f"expected VDF object: {key}")
                visit(current, value, depth + 1)
            elif isinstance(current, Object):
                raise ValueError(f"expected VDF string: {key}")
            elif current.value != value:
                edits.append((current.start, current.end, quote(value)))
        if missing:
            edits.append((node.end, node.end, "\n" + render(missing, depth)))

    visit(root, values)
    for start, end, content in sorted(edits, reverse=True):
        text = text[:start] + content + text[end:]
    return text


def prepare(path: Path, desired: dict):
    before = path.read_bytes().decode("utf-8") if path.exists() else ""
    return path, before, merge(before, desired)


def write(change):
    path, before, after = change
    if before == after:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    mode = stat.S_IMODE(path.stat().st_mode) if path.exists() else 0o600
    fd, temporary = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.")
    try:
        with os.fdopen(fd, "w") as target:
            os.fchmod(target.fileno(), mode)
            target.write(after)
            target.flush()
            os.fsync(target.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main():
    if len(sys.argv) != 3 or not re.fullmatch(r"[1-9][0-9]*", sys.argv[2]):
        print("usage: steam-vdf-settings <steam-home> <appid>", file=sys.stderr)
        return 2
    home, appid = Path(sys.argv[1]), sys.argv[2]
    mapping = {key: {"name": TOOL, "config": "", "priority": "250"} for key in ("0", appid)}
    # These are native Steam config.vdf keys, not Korri settings. Korri Seat
    # already provides Xbox pads; Steam must not remap or synthesize another pad.
    settings = {
        "CompatToolMapping": mapping,
        "SteamController_XBoxSupport": "0",
        "SteamController_GenericSupport": "0",
    }
    try:
        changes = [prepare(home / "config/config.vdf", {
            "InstallConfigStore": {"Software": {"Valve": {"Steam": settings}}},
        })]
        users = sorted(path for path in (home / "userdata").glob("*")
                       if path.is_dir() and path.name.isdecimal() and int(path.name) > 0)
        for user in users:
            seeds = {f"Deck_ConfiguratorInterstitialsVersionSeen_{key}": "99" for key in ONCE + PER_GAME}
            seeds.update({f"Deck_ConfiguratorInterstitialsCheckbox_{key}": "1" for key in PER_GAME})
            seeds["apps"] = {appid: {f"{appid}_eula_{index}": "1" for index in range(3)}}
            changes.append(prepare(user / "config/localconfig.vdf", {
                "UserLocalConfigStore": {"Software": {"Valve": {"Steam": seeds}}},
            }))
        # Validate all input before changing any file.
        for change in changes:
            write(change)
        if not users:
            print("steam-session: settings: no login; userdata seeds skipped", file=sys.stderr)
    except (OSError, ValueError) as error:
        print(f"steam-session: settings failed: {error}", file=sys.stderr)
        return 126
    return 0


if __name__ == "__main__":
    sys.exit(main())
