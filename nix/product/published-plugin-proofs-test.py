#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#nix --command python3
"""Check actual catalog archive proofs against unsigned image registration."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def run(*args, success=True, **kwargs):
    result = subprocess.run(args, capture_output=True, text=True, **kwargs)
    assert (result.returncode == 0) == success, (args, result.stdout, result.stderr)
    return result


def main():
    metadata, registration = map(Path, sys.argv[1:3])
    roots = sys.argv[3:]
    with tempfile.TemporaryDirectory() as temporary:
        work = Path(temporary)
        env = {**os.environ, "HOME": str(work), "NIX_REMOTE": f"local?state={work}/state"}
        with registration.open() as source:
            run("nix-store", "--load-db", stdin=source, env=env)
        nix = ["nix", "--offline", "--extra-experimental-features", "nix-command",
               "--option", "substituters", "", "--option", "extra-substituters", ""]
        info = json.loads(run(*nix, "path-info", "--recursive", "--json", *roots, env=env).stdout)
        entries = info.values() if isinstance(info, dict) else info
        assert all(not value.get("signatures") and not value.get("ultimate") for value in entries)
        run(str(metadata / "verify"), env=env)
        verified = json.loads(run(*nix, "path-info", "--recursive", "--json", *roots, env=env).stdout)
        if not isinstance(verified, dict):
            verified = {entry["path"]: entry for entry in verified}
        assert all(verified[root]["signatures"] for root in roots)
        # Slim archives must not turn root-registered upstream paths into paths
        # authorized by the publisher. The actual closure has both classes.
        assert any(not entry.get("signatures") for entry in verified.values())
        run(*nix, "store", "verify", "--recursive", "--no-trust", *roots, env=env)
        # Missing private proof must fail even though valid signatures have
        # already been registered locally. No mutable cache can fill the gap.
        copied = work / "missing-proof"
        shutil.copytree(metadata, copied, symlinks=True)
        for directory in [copied, *[path for path in copied.rglob('*') if path.is_dir() and not path.is_symlink()]]:
            directory.chmod(0o755)
        root = next(path for path in roots if (copied / "korri-plugin-mgba").exists() and
                    (copied / "korri-plugin-mgba" / (Path(path).name[:32] + ".narinfo")).exists())
        (copied / "korri-plugin-mgba" / (Path(root).name[:32] + ".narinfo")).unlink()
        failure = run(str(copied / "verify"), success=False, env=env)
        assert root in failure.stderr, failure.stderr
        print(f"PASS: {len(roots)} published roots; unsigned image registration; bound-key private proofs; unsigned upstream paths; missing private proof refused")


if __name__ == "__main__":
    main()
