#!/usr/bin/env python3
"""Emit a canonical semantic map for an Android shared_prefs directory."""

from __future__ import annotations

import json
import math
import sys
import xml.etree.ElementTree as ET
from pathlib import Path


def fail(message: str) -> "NoReturn":
    raise ValueError(message)


def typed_value(element: ET.Element) -> dict[str, object]:
    tag = element.tag
    if tag == "string":
        return {"type": "string", "value": element.text or ""}
    if tag == "boolean":
        value = element.attrib.get("value")
        if value not in {"true", "false"}:
            fail(f"invalid boolean value: {value!r}")
        return {"type": "boolean", "value": value == "true"}
    if tag in {"int", "long"}:
        return {"type": tag, "value": int(element.attrib["value"])}
    if tag == "float":
        value = float(element.attrib["value"])
        if not math.isfinite(value):
            fail("non-finite float is not a SharedPreferences semantic value")
        return {"type": "float", "value": value}
    if tag == "set":
        values = []
        for child in element:
            if child.tag != "string" or child.attrib:
                fail("SharedPreferences sets may contain only strings")
            values.append(child.text or "")
        return {"type": "string-set", "value": sorted(values)}
    fail(f"unsupported SharedPreferences type: {tag}")


def snapshot(directory: Path) -> dict[str, dict[str, object]]:
    if not directory.exists():
        return {}
    if not directory.is_dir():
        fail(f"not a directory: {directory}")

    result: dict[str, dict[str, object]] = {}
    for path in sorted(directory.iterdir(), key=lambda item: item.name):
        if not path.is_file() or path.suffix != ".xml":
            fail(f"unexpected SharedPreferences entry: {path.name}")
        root = ET.parse(path).getroot()
        if root.tag != "map" or root.attrib:
            fail(f"invalid SharedPreferences map root: {path.name}")
        for element in root:
            name = element.attrib.get("name")
            allowed_attributes = {"name"} if element.tag in {"string", "set"} else {"name", "value"}
            if not name or set(element.attrib) != allowed_attributes:
                fail(f"invalid SharedPreferences entry in {path.name}")
            identity = f"{path.name}:{name}"
            if identity in result:
                fail(f"duplicate SharedPreferences key: {identity}")
            result[identity] = typed_value(element)
    return result


def main() -> None:
    if len(sys.argv) != 2:
        print("usage: shared-preferences-snapshot.py <shared-prefs-directory>", file=sys.stderr)
        raise SystemExit(2)
    try:
        value = snapshot(Path(sys.argv[1]))
    except (ET.ParseError, KeyError, OSError, ValueError) as error:
        print(f"could not snapshot SharedPreferences: {error}", file=sys.stderr)
        raise SystemExit(1) from error
    json.dump(value, sys.stdout, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
