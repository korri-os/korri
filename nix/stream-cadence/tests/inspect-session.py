#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 openssh
"""Read journal evidence for a recorded session without changing the device."""

import argparse
import json
from pathlib import Path
import shlex
import subprocess

p = argparse.ArgumentParser()
p.add_argument("target")
p.add_argument("--ssh-port", required=True)
p.add_argument("--result", type=Path, required=True)
p.add_argument("--since", required=True)
p.add_argument("--until", required=True)
p.add_argument("--out", type=Path, required=True)
a = p.parse_args()
facts = json.loads(a.result.read_text())["discovered"]
command = shlex.join(
    [
        "journalctl",
        "-u",
        facts["sunshine_unit"],
        "-b",
        "--since",
        a.since,
        "--until",
        a.until,
        "-o",
        "short-iso-precise",
        "--no-pager",
    ]
)
r = subprocess.run(
    [
        "ssh",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-p",
        a.ssh_port,
        a.target,
        command,
    ],
    capture_output=True,
    text=True,
    check=True,
    timeout=30,
)
a.out.parent.mkdir(parents=True, exist_ok=True)
a.out.write_text(r.stdout)
print(r.stdout, flush=True)
