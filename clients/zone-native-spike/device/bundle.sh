#!/usr/bin/env nix-shell
#! nix-shell -i bash -p python3 binutils nix
# shellcheck shell=bash
# Usage: bundle.sh STORE_PACKAGE BINARY [OUTPUT_DIRECTORY]
set -euo pipefail
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
    echo "usage: $0 STORE_PACKAGE BINARY [OUTPUT_DIRECTORY]" >&2
    exit 2
fi
python3 - "$@" <<'PY'
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile

package = Path(sys.argv[1]).resolve()
app = sys.argv[2]
if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", app):
    raise SystemExit("binary must be a basename")
out = Path(sys.argv[3] if len(sys.argv) > 3 else f"{app}-bundle").resolve()
archive = out.with_name(out.name + ".tar.gz")
if out.exists() or archive.exists():
    raise SystemExit("output directory or archive already exists; choose a new output")
binary = package / "bin" / app

def elf(path, flag):
    return subprocess.check_output(["readelf", flag, str(path)], text=True)

if "AArch64" not in elf(binary, "-h"):
    raise SystemExit("expected an aarch64 ELF executable")
closure = subprocess.check_output(["nix-store", "--query", "--requisites", str(package)], text=True).splitlines()
index = {}
for root in closure:
    for directory, _, files in os.walk(root):
        for name in files:
            if ".so" in name:
                index.setdefault(name, []).append(Path(directory) / name)

interpreter = re.search(r"Requesting program interpreter: (.*?)\]", elf(binary, "-l"))
if not interpreter:
    raise SystemExit("missing ELF interpreter")
loader = Path(interpreter[1])
# wayland-sys dlopen backend, winit cursor/xkbcommon-dl, and fontique's
# fontconfig-dlopen. See README.md for source audit and optional libraries.
runtime = os.environ.get("BUNDLE_DLOPEN", "libwayland-client.so.0 libwayland-cursor.so.0 libxkbcommon.so.0 libfontconfig.so.1").split()
(out / "bin").mkdir(parents=True)
(out / "lib").mkdir()
shutil.copy2(binary, out / "bin" / app)
queue = [(loader, binary)]
queue += [(name, binary) for name in re.findall(r"Shared library: \[(.*?)\]", elf(binary, "-d"))]
queue += [(name, binary) for name in runtime]
seen = set()
manifest = []
while queue:
    item, caller = queue.pop(0)
    name = Path(item).name
    if name in seen:
        continue
    seen.add(name)
    if isinstance(item, Path):
        source = item
    else:
        paths = re.findall(r"Library (?:rpath|runpath): \[(.*?)\]", elf(caller, "-d"))
        candidates = [Path(directory.replace("${ORIGIN}", str(caller.parent)).replace("$ORIGIN", str(caller.parent))) / item
                      for value in paths for directory in value.split(":")]
        source = next((p for p in candidates if p.is_file()), None)
        if source is None:
            choices = index.get(item, [])
            if not choices:
                raise SystemExit(f"library not in package closure: {item}")
            source = choices[0]
            if any(p.read_bytes() != source.read_bytes() for p in choices[1:]):
                raise SystemExit(f"ambiguous library outside caller RUNPATH: {item}")
    shutil.copy2(source, out / "lib" / name, follow_symlinks=True)
    manifest.append(f"{name}\t{source}\n")
    queue += [(name, source) for name in re.findall(r"Shared library: \[(.*?)\]", elf(source, "-d"))]
(out / "libraries.tsv").write_text("".join(sorted(manifest)))
# winit creates an XKB context even without text input. libxkbcommon embeds
# its Nix data path; retain the rules/symbols too, not just the ELF library.
xkb_setup = ""
if "libxkbcommon.so.0" in seen:
    roots = [Path(root) / "etc/X11/xkb" for root in closure if (Path(root) / "etc/X11/xkb").is_dir()]
    if len(roots) != 1:
        raise SystemExit(f"expected one xkeyboard-config data directory, found {len(roots)}")
    shutil.copytree(roots[0], out / "share" / "xkb", symlinks=False)
    xkb_setup = 'XKB_CONFIG_ROOT="$PWD/share/xkb"; export XKB_CONFIG_ROOT\n'
(out / "run.sh").write_text(
    '#!/bin/sh\nset -eu\ncase $0 in */*) cd -- "${0%/*}" ;; *) echo "invoke run.sh with a path" >&2; exit 2 ;; esac\n'
    + xkb_setup
    + f'exec ./lib/ld-linux-aarch64.so.1 --inhibit-cache --library-path ./lib ./bin/{app} "$@"\n'
)
(out / "run.sh").chmod(0o755)
with tarfile.open(archive, "w:gz") as tar:
    tar.add(out, arcname=out.name)
print(out)
print(archive)
PY
