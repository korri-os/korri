"""A prepare-program fixture. PREPARE_BEHAVIOR selects the outcome."""
import os
from pathlib import Path
import sys

name = Path(sys.argv[0]).name
behavior = os.environ.get("PREPARE_BEHAVIOR", "ready")
if behavior == f"fail:{name}":
    print(f"{name}: download failed", file=sys.stderr)
    sys.exit(1)
cache = Path(sys.argv[1])
print(f"{name}: fixture ready", file=sys.stderr)
# Only the last stdout line names the ready directory.
print("earlier stdout line")
if behavior == "no-directory":
    print(cache / "missing")
    sys.exit(0)
ready = cache / ("fex/rootfs" if name == "fex-prepare" else "proton/cachyos") / "fixture"
ready.mkdir(parents=True, exist_ok=True)
print(ready)
