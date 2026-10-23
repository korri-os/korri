#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 openssh
"""Explicit manual acceptance preparation. Never called by the cadence tool."""

import argparse
import json
from pathlib import Path
import shlex
import subprocess
import time

p = argparse.ArgumentParser()
p.add_argument("target")
p.add_argument("--ssh-port", required=True)
p.add_argument("--restart-sunshine-once", action="store_true")
p.add_argument("--blank", action="store_true")
p.add_argument("--out", type=Path, required=True)
a = p.parse_args()


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
        timeout=40,
    ).strip()


if remote(
    "systemctl list-units 'korri-game-*' --state=active,activating,deactivating --no-legend --plain"
):
    raise SystemExit("Close the owner game first; nothing was changed.")
pid = remote("systemctl show korri-compositor -p MainPID --value")
executable = remote("readlink -f /proc/" + pid + "/exe")
env = dict(
    line.split("=", 1)
    for line in remote('tr "\\0" "\\n" </proc/' + pid + "/environ").splitlines()
    if "=" in line
)
sm = str(Path(executable).with_name("swaymsg"))
base = shlex.join([sm, "-s", env["SWAYSOCK"], "-r"])
display = next(
    row for row in json.loads(remote(base + " -t get_outputs")) if row["active"]
)
record = {
    "at": time.strftime("%FT%TZ", time.gmtime()),
    "before_display": display,
    "manual_sunshine_restart": a.restart_sunshine_once,
    "manual_blank": a.blank,
}
if a.blank:
    record["reply"] = remote(
        base + " " + shlex.quote("output " + json.dumps(display["name"]) + " power off")
    )
else:
    remote(base + " " + shlex.quote("seat * idle_notify"))
    for _ in range(100):
        rows = json.loads(remote(base + " -t get_outputs"))
        if next(row for row in rows if row["name"] == display["name"])["power"]:
            break
        time.sleep(0.1)
    else:
        raise SystemExit("Panel did not wake; did not restart Sunshine.")
if a.restart_sunshine_once:
    if a.out.exists():
        raise SystemExit(
            "This preparation record exists; refusing a second manual restart."
        )
    status = json.loads(remote("korri-plugin status @korri:sunshine"))
    manifest = json.loads(
        remote("cat " + shlex.quote(status["package"] + "/manifest.json"))
    )
    services = []
    for name in manifest["services"]:
        if name.endswith(".socket"):
            continue
        candidate = remote(
            "systemctl show " + shlex.quote(name) + " -p MainPID --value"
        )
        if candidate != "0" and "KORRI_CERTIFICATE_CONTROL_PATH=" in remote(
            "systemctl show " + shlex.quote(name) + " -p Environment --value"
        ):
            services.append(name)
    if len(services) != 1:
        raise SystemExit(
            "Cannot discover one installed Sunshine service; did not restart it."
        )
    record["sunshine_unit"] = services[0]
    record["before_pid"] = remote(
        "systemctl show " + shlex.quote(services[0]) + " -p MainPID --value"
    )
    # Persist the intent first so a failed probe cannot authorize a second restart.
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(record, indent=2) + "\n")
    record["restart_reply"] = remote("systemctl restart " + shlex.quote(services[0]))
    record["after_pid"] = remote(
        "systemctl show " + shlex.quote(services[0]) + " -p MainPID --value"
    )
    stamp = remote(
        "systemctl show "
        + shlex.quote(services[0])
        + " -p ExecMainStartTimestamp --value"
    )
    record["after_start_timestamp"] = stamp
    for _ in range(100):
        journal = remote(
            "journalctl -u "
            + shlex.quote(services[0])
            + " -b --since "
            + shlex.quote(stamp)
            + " -o cat --no-pager"
        )
        record["fresh_journal"] = journal
        if "Found " in journal and " encoder: " in journal:
            record["working_encoder_confirmed"] = True
            break
        if "Video failed to find working encoder" in journal:
            record["working_encoder_confirmed"] = False
            break
        time.sleep(0.25)
    else:
        record["working_encoder_confirmed"] = False
record["after_display"] = json.loads(remote(base + " -t get_outputs"))
a.out.parent.mkdir(parents=True, exist_ok=True)
a.out.write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps(record), flush=True)
