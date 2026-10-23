#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 openssh
"""Read-only acceptance discovery; target and port are caller inputs."""

import argparse
import pathlib
import subprocess

p = argparse.ArgumentParser()
p.add_argument("target")
p.add_argument("--ssh-port", required=True)
p.add_argument("--out", type=pathlib.Path, required=True)
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=True)
commands = {
    "identity.txt": "uname -m; cat /proc/device-tree/model; cat /etc/os-release; readlink -f /run/current-system; korri-plugin status @korri:sunshine",
    "units.txt": "systemctl list-units '*sunshine*' --all --no-legend --plain; systemctl show korri-compositor korrid -p Id -p MainPID; command -v python3; ip -br addr",
    "sunshine-unit.txt": "systemctl show korri-sunshine -p MainPID -p Environment -p ExecStart -p User -p Group -p WorkingDirectory -p FragmentPath -p ExecMainStartTimestamp; systemctl cat korri-sunshine",
    "journal.txt": "journalctl -u korri-sunshine -b -o cat --no-pager",
    "journal-cursor.txt": "journalctl -u korri-sunshine -b -n 0 --show-cursor --no-pager",
    "sunshine-process.txt": 'pid=$(systemctl show korri-sunshine -p MainPID --value); readlink -f /proc/$pid/exe; tr "\\0" " " </proc/$pid/cmdline; echo; cat /proc/$pid/status',
    "compositor.txt": 'pid=$(systemctl show korri-compositor -p MainPID --value); cat /proc/$pid/status; tr "\\0" "\\n" </proc/$pid/environ; readlink -f /proc/$pid/exe',
    "outputs.json": 'pid=$(systemctl show korri-compositor -p MainPID --value); sm=$(dirname "$(readlink -f /proc/$pid/exe)")/swaymsg; sock=$(tr "\\0" "\\n" </proc/$pid/environ | sed -n "s/^SWAYSOCK=//p"); "$sm" -s "$sock" -t get_outputs',
}
for name, command in commands.items():
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
        timeout=30,
    )
    (a.out / name).write_text(r.stdout.replace("\0", "\n"))
    print(name, r.returncode, r.stderr, flush=True)
    print(r.stdout, flush=True)
