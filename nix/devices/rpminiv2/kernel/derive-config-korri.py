#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3
"""Apply the reviewed Kconfig delta to the unchanged ROCKNIX configuration."""

import argparse
from pathlib import Path
import re

SETTING = re.compile(r"^(CONFIG_[A-Za-z0-9_]+)=(.+)$")
DISABLED = re.compile(r"^# (CONFIG_[A-Za-z0-9_]+) is not set$")


def setting(line):
    if match := SETTING.fullmatch(line):
        return match[1], match[2]
    if match := DISABLED.fullmatch(line):
        return match[1], "n"
    return None


def settings(text):
    values = {}
    for line in text.splitlines():
        if item := setting(line):
            name, value = item
            if name in values:
                raise ValueError(f"duplicate Kconfig symbol: {name}")
            values[name] = value
        elif line.startswith("# CONFIG_") or (
            line.strip() and not line.startswith("#")
        ):
            raise ValueError(f"malformed Kconfig setting: {line}")
    return values


def render(name, value):
    return f"# {name} is not set" if value == "n" else f"{name}={value}"


def derive(baseline, delta):
    settings(baseline)
    overrides = settings(delta)
    lines = []
    for line in baseline.splitlines():
        item = setting(line)
        if item and item[0] in overrides:
            name = item[0]
            line = render(name, overrides.pop(name))
        lines.append(line)
    lines.extend(render(name, value) for name, value in overrides.items())
    return "\n".join(lines) + "\n"


def differences(expected, actual):
    # Kconfig omits disabled symbols with disabled dependencies. An absent
    # symbol and an explicit 'not set' both mean n; every other value is exact.
    return [
        f"{name}: expected {expected.get(name, 'n')}, got {actual.get(name, 'n')}"
        for name in sorted(expected.keys() | actual.keys())
        if expected.get(name, "n") != actual.get(name, "n")
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-directory", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()
    root = args.source_directory
    output = derive(
        (root / "config").read_text(), (root / "config-korri.delta").read_text()
    )
    (root / "config-korri").write_text(output)


if __name__ == "__main__":
    main()
