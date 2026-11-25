#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#openssh --command python3
"""Read-only device facts before attempting the #616 installed-viewer proof."""

import argparse
import json
import pathlib
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("device", choices=["tabs6", "op6"])
parser.add_argument("--phase", choices=["preflight", "postflight"], default="preflight")
parser.add_argument(
    "--host", help="Explicit SSH target instead of the device's .local name"
)
args = parser.parse_args()
script = r"""
set -eu
printf '== UTC ==\n'
date -u --iso-8601=seconds
printf '== system ==\n'
readlink -f /run/current-system
printf '== thermal zones (millidegrees C) ==\n'
for zone in /sys/class/thermal/thermal_zone*; do
  printf '%s ' "$zone"
  cat "$zone/type" "$zone/temp" | tr '\n' ' '
  printf '\n'
done
printf '== decoder nodes ==\n'
for node in /sys/class/video4linux/video*; do
  printf '%s ' "$node"
  cat "$node/name"
done
printf '== running services ==\n'
systemctl is-active korrid korri-shell || true
printf '== runtime session ==\n'
id korri
systemctl show korri-compositor -p User -p Environment
printf '== screenshot tool ==\n'
command -v grim || true
printf '== input nodes ==\n'
find /dev/input -maxdepth 2 -type c -print
printf '== audio sockets ==\n'
find /run/user/1000 -maxdepth 3 -type s -print || true
printf '== audio cards ==\n'
cat /proc/asound/cards || true
printf '== CLI ==\n'
korri --help
korri plugins --help
korri plugins list --help
korri plugins publishers --help
korri play --help
printf '== publishers ==\n'
korri --json plugins publishers
printf '== installed plugins ==\n'
korri --json plugins list || true
printf '== session status ==\n'
korri --json status || true
printf '== existing receipt directories ==\n'
find /var/lib/korri-plugin-host -maxdepth 2 -name selection.json -print
printf '== Nix device policy ==\n'
nix config show | grep -E '^(builders|max-jobs|fallback|require-sigs|substituters|trusted-public-keys) ='
"""
result = subprocess.run(
    [
        "ssh",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=15",
        "-p",
        "2222",
        args.host or f"root@{args.device}.local",
        "bash -s",
    ],
    input=script,
    text=True,
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
    check=False,
)
out = pathlib.Path(__file__).parent / args.device
out.mkdir(parents=True, exist_ok=True)
if "== session status ==\n" not in result.stdout:
    # The script stopped before status, so no private status fields were read.
    (out / f"{args.phase}.txt").write_text(result.stdout)
    print(result.stdout, end="")
    raise SystemExit(result.returncode or 1)
# Keep only the session observation. Status also includes nearby Wi-Fi names
# and public identity keys; those facts are not part of this picture proof.
head, tail = result.stdout.split("== session status ==\n", 1)
status, rest = tail.split("== existing receipt directories ==\n", 1)
responses = json.loads(status.strip())
session = [
    response for response in responses if response.get("_tag") == "app.session.status"
]
output = head + "== session status ==\n" + json.dumps(session, indent=2) + "\n"
output += "== existing receipt directories ==\n" + rest
(out / f"{args.phase}.txt").write_text(output)
print(output, end="")
raise SystemExit(result.returncode)
