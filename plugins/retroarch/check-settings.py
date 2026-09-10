#!/usr/bin/env python3
"""Kind-owned check: legacy rendered cfg types against this program's source.

This is source evidence, not proof that an optional feature was compiled in.
Unknown keys, parser forms and type conflicts fail closed. No version ranges.
"""
import argparse
import json
from pathlib import Path
import re
import sys

TYPES = {
    "BOOL": "Boolean",
    "INT": "Number",
    "UINT": "Number",
    "FLOAT": "Number",
    "DOUBLE": "Number",
    "SIZE": "Number",
    "SIZE_T": "Number",
    "ARRAY": "String",
    "PATH": "String",
    "STRING": "String",
}


def source_types(source):
    # Remove comments without treating // inside quoted paths as a comment.
    source = re.sub(
        r'"(?:\\.|[^"\\])*"|/\*.*?\*/|//[^\n]*',
        lambda match: match[0] if match[0].startswith('"') else " ",
        source,
        flags=re.S,
    )
    found = {}
    patterns = [
        r'\bSETTING_(\w+)\s*\(\s*"([A-Za-z0-9_]+)"',
        r'\bconfig_get_(\w+)\s*\(\s*\w+\s*,\s*"([A-Za-z0-9_]+)"',
        r'\bCONFIG_GET_(\w+)_BASE\s*\([^;]*?,\s*"([A-Za-z0-9_]+)"\s*\)',
    ]
    for pattern in patterns:
        for native_type, key in re.findall(pattern, source):
            found.setdefault(key, set()).add(TYPES.get(native_type.upper()))
    return {key: next(iter(types)) for key, types in found.items() if len(types) == 1}


def check(table, source, reserved):
    native = source_types(source)
    accepted = {}
    omitted = {}
    for key, expected in sorted(table.items()):
        if expected not in {"Boolean", "Number", "String"}:
            raise ValueError(f"invalid kind table type for {key}: {expected}")
        if key in reserved:
            omitted[key] = "reserved by the launch callback"
        elif native.get(key) != expected:
            omitted[key] = f"expected {expected}, source has {native.get(key, 'no verified key')}"
        else:
            accepted[key] = expected
    return accepted, omitted


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--configuration", type=Path, required=True)
    parser.add_argument("--table", type=Path, required=True)
    parser.add_argument("--callback", type=Path, required=True)
    parser.add_argument("--program", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    # The callback is the existing owner of the security restriction. Never
    # maintain a second list in generated packaging metadata.
    reserved_block = re.search(r"const reservedKeys = \[(.*?)\]", args.callback.read_text(), re.S)
    if reserved_block is None:
        raise ValueError("callback reserved-key declaration was not found")
    reserved = set(re.findall(r'"([A-Za-z0-9_]+)"', reserved_block[1]))
    accepted, omitted = check(
        json.loads(args.table.read_text()), args.configuration.read_text(), reserved
    )
    if not accepted:
        raise ValueError("no kind settings verified against the program source")
    for key, reason in omitted.items():
        print(f"RetroArch {args.version}: omit {key}: {reason}", file=sys.stderr)
    args.output.write_text(json.dumps({
        "program": args.program, "version": args.version, "keys": accepted,
    }, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
