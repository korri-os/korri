#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 openssh
"""Independent read-only check of PIDs, run folder, and persisted certificate.

Sunshine's existing state producer is korri_certificate_control.cpp:
build_state_document writes root.uniqueid and root.named_devices[*].cert.
Never print the document, other client names, or certificate contents.
"""

import argparse
import hashlib
import json
from pathlib import Path
import shlex
import ssl
import subprocess

p = argparse.ArgumentParser()
p.add_argument("target")
p.add_argument("--ssh-port", required=True)
p.add_argument("--result", type=Path, required=True)
p.add_argument("--certificate", type=Path, required=True)
p.add_argument("--out", type=Path, required=True)
a = p.parse_args()
facts = json.loads(a.result.read_text())["discovered"]


def remote(command):
    return subprocess.check_output(
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
        text=True,
        timeout=30,
    ).strip()


def fingerprint(certificate):
    return hashlib.sha256(ssl.PEM_cert_to_DER_cert(certificate)).hexdigest()


wanted = fingerprint(a.certificate.read_text())
state = json.loads(remote("cat " + shlex.quote(facts["sunshine_state_path"])))["root"]
matching = sum(fingerprint(entry["cert"]) == wanted for entry in state["named_devices"])
pids = {}
for key, unit in [
    ("sunshine", facts["sunshine_unit"]),
    ("compositor", "korri-compositor"),
    ("korrid", "korrid"),
]:
    pids[key] = int(
        remote("systemctl show " + shlex.quote(unit) + " -p MainPID --value")
    )
folder = facts["restore"]["run_folder"]
folder_absent = (
    remote(
        "if test -e "
        + shlex.quote(folder)
        + "; then echo present; else echo absent; fi"
    )
    == "absent"
)
record = {
    "client_certificate_sha256": wanted,
    "matching_persisted_grants": matching,
    "sunshine_identity_retained": state["uniqueid"] == facts["host_uuid"],
    "pids": pids,
    "pids_unchanged": all(
        pid == facts["processes"][key]["pid"] for key, pid in pids.items()
    ),
    "run_folder_absent": folder_absent,
}
record["pass"] = (
    matching == 0
    and record["sunshine_identity_retained"]
    and record["pids_unchanged"]
    and folder_absent
)
a.out.write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps(record), flush=True)
raise SystemExit(0 if record["pass"] else 1)
