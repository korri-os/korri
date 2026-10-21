#!/usr/bin/env python3
"""Copy real declarations from an immutable korri-os/plugins Git tree.

Usage: update-first-party-fixtures.py PUBLISHER_CHECKOUT FULL_COMMIT_SHA
Never edit the generated declarations or their checksums by hand.
"""
import hashlib
from pathlib import Path
import subprocess
import sys


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    checkout, revision = sys.argv[1:]
    resolved = subprocess.check_output(
        ["git", "-C", checkout, "rev-parse", "--verify", f"{revision}^{{commit}}"],
        text=True,
    ).strip()
    if revision != resolved:
        raise SystemExit("Use the full immutable commit SHA, not a branch or tag")
    # Read all sources before changing the snapshot. No working-tree bytes enter it.
    sources = {
        f"{name}/plugin.ts": subprocess.check_output(
            ["git", "-C", checkout, "show", f"{revision}:plugins/{name}/plugin.ts"]
        )
        for name in ("fex", "proton", "steam", "pico8", "homebrewhub", "retrobrews")
    }
    root = Path(__file__).parent / "fixtures/first-party"
    for name, data in sources.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    (root / "REVISION").write_text(revision + "\n")
    (root / "SHA256SUMS").write_text("".join(
        f"{hashlib.sha256(data).hexdigest()}  {name}\n"
        for name, data in sources.items()
    ))


if __name__ == "__main__":
    main()
