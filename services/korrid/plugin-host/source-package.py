#!/usr/bin/env python3
"""Copy the closed plugin source graph and generate its manifest inventory."""

import json
import os
import shutil
import stat
import sys
from pathlib import Path, PurePosixPath

SOURCE_ENTRIES = 512
SOURCE_BYTES = 4 * 1024 * 1024
# The host charges each relative name more than once while it validates and
# resolves the graph. These conservative build limits leave ample room within
# its 256 KiB aggregate path budget and 8192 traversal-step budget, including
# the fixed /nix/store/<output> root.
SOURCE_RELATIVE_PATH_BYTES = 64 * 1024
SOURCE_RELATIVE_COMPONENTS = 4096
ALLOWED_SUFFIXES = {".ts", ".js", ".mjs", ".cjs", ".json"}


def fail(message: str) -> None:
    raise SystemExit(message)


def package(source: Path, output: Path, manifest_base: Path) -> None:
    if not source.is_dir():
        fail("plugin source must be a directory containing plugin.ts")
    if source.is_symlink():
        fail("plugin source root must not be a symlink")

    selected: list[str] = []
    total_bytes = 0
    total_path_bytes = 0
    total_components = 0
    for directory, directories, files in os.walk(source, followlinks=False):
        base = Path(directory)
        for name in [*directories, *files]:
            candidate = base / name
            if candidate.is_symlink():
                fail(
                    "plugin source must not contain symlinks: "
                    f"{candidate.relative_to(source)}"
                )
        for name in files:
            candidate = base / name
            relative = PurePosixPath(candidate.relative_to(source).as_posix())
            if relative.as_posix() == "manifest.json" or candidate.suffix not in ALLOWED_SUFFIXES:
                continue
            mode = candidate.stat(follow_symlinks=False).st_mode
            if not stat.S_ISREG(mode):
                fail(f"plugin source must be a regular file: {relative}")
            relative_name = relative.as_posix()
            selected.append(relative_name)
            total_bytes += candidate.stat(follow_symlinks=False).st_size
            total_path_bytes += len(relative_name.encode("utf-8"))
            total_components += len(relative.parts)

    selected.sort()
    if "plugin.ts" not in selected:
        fail("plugin source directory must contain plugin.ts")
    if len(selected) > SOURCE_ENTRIES:
        fail(f"plugin source inventory exceeds {SOURCE_ENTRIES} entries")
    if total_bytes > SOURCE_BYTES:
        fail("plugin source inventory exceeds 4 MiB")
    if total_path_bytes > SOURCE_RELATIVE_PATH_BYTES:
        fail("plugin source inventory exceeds 64 KiB of relative paths")
    if total_components > SOURCE_RELATIVE_COMPONENTS:
        fail("plugin source inventory exceeds 4096 relative path components")

    output.mkdir(parents=True)
    for name in selected:
        destination = output / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source / name, destination, follow_symlinks=False)

    manifest = json.loads(manifest_base.read_text())
    manifest["entry"] = "plugin.ts"
    manifest["sources"] = selected
    (output / "manifest.json").write_text(
        json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n"
    )


def main() -> None:
    if len(sys.argv) != 4:
        fail("usage: source-package.py SOURCE OUTPUT MANIFEST_BASE")
    package(Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]))


if __name__ == "__main__":
    main()
