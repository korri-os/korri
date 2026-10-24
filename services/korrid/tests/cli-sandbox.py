#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 util-linux
"""Real negative connection proof using systemd's own inaccessible inodes.

The Rust test passes only paths emitted by the game's sandbox builder. This
isolates the mount change in a user/mount namespace; it changes no host mount.
Exit 77 means this host cannot provide the required Linux namespace/inodes.
"""
import os
from pathlib import Path
import socket
import subprocess
import sys


def connects(path):
    with socket.socket(socket.AF_UNIX) as stream:
        try:
            stream.connect(path)
            return True
        except OSError:
            return False


def main():
    if sys.argv[1] == "--inside":
        path, parent, mask = sys.argv[2:]
        source = f"/run/systemd/inaccessible/{mask}"
        target = path if mask == "sock" else parent
        result = subprocess.run(["mount", "--bind", source, target], capture_output=True)
        if result.returncode:
            return 77
        assert not connects(path), "game namespace reached the person socket"
        if mask == "dir":
            try:
                os.listdir(parent)
            except PermissionError:
                pass
            else:
                raise AssertionError("game namespace can inspect the person socket parent")
        return 0
    path, parent = sys.argv[1:]
    if not all(Path(f"/run/systemd/inaccessible/{kind}").exists() for kind in ["sock", "dir"]):
        return 77
    assert connects(path), "fixture socket must work outside the game namespace"
    for mask in ["sock", "dir"]:
        result = subprocess.run(["unshare", "--user", "--map-root-user", "--mount", "--propagation", "private", sys.executable, __file__, "--inside", path, parent, mask], capture_output=True)
        if result.returncode == 77 or b"Operation not permitted" in result.stderr:
            return 77
        if result.returncode:
            sys.stderr.buffer.write(result.stderr)
            return result.returncode
    assert connects(path), "namespace test changed the host socket"
    return 0


if __name__ == "__main__":
    sys.exit(main())
