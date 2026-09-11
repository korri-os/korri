#!/usr/bin/env nix
#! nix shell nixpkgs#python3 --command python3
"""Refuse probe dependencies that differ from the deployed engine's lock."""

from pathlib import Path
import sys
import tomllib


def package_key(package):
    return package["name"], package["version"], package.get("checksum")


def main(original_path, probe_path):
    original = tomllib.loads(Path(original_path).read_text())["package"]
    probe = tomllib.loads(Path(probe_path).read_text())["package"]
    known = {package_key(package) for package in original}
    unknown = [
        package_key(package)
        for package in probe
        if package["name"] != "retroarch-effect-quickjs-probe"
        and package_key(package) not in known
    ]
    if unknown:
        raise SystemExit(f"Probe dependency drift: {unknown}")


if __name__ == "__main__":
    main(*sys.argv[1:])
