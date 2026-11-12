"""Stand-ins for the three programs that the sender fixture starts.

The first argument names the role; a wrapper script next to this file's
caller supplies it. Behaviour comes from files in the wrapper's directory:

- `hardware-nodes` lists the render nodes on which "Sunshine" finds a
  hardware encoder; on any other node it falls back to its CPU encoder.
- `arguments.log` receives one line for each start, so a test can see what
  was visible in the process list.
"""

import json
import os
from pathlib import Path
import re
import signal
import socket
import sys
import time

FIXTURES = Path(__file__).resolve().parent / "fixtures" / "sender"


def stop(_signum: int, _frame: object) -> None:
    raise SystemExit(0)


signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
knobs, role, arguments = Path(sys.argv[1]), sys.argv[2], sys.argv[3:]
with (knobs / "arguments.log").open("a") as record:
    record.write(json.dumps([role, *arguments]) + "\n")
if role == "sway":
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as display:
        display.bind(str(Path(os.environ["XDG_RUNTIME_DIR"]) / "wayland-1"))
        display.listen(1)
        while True:
            time.sleep(1)
elif role == "swaymsg":
    # The mode that the fixture wrote into its compositor's configuration.
    root = Path(arguments[arguments.index("-s") + 1]).parents[1]
    found = re.search(r"mode (\d+)x(\d+)@([\d.]+)Hz", (root / "sway.conf").read_text())
    assert found
    width, height = int(found.group(1)), int(found.group(2))
    print(
        json.dumps(
            [
                {
                    "name": "HEADLESS-1",
                    "active": True,
                    "current_mode": {
                        "width": width,
                        "height": height,
                        "refresh": round(float(found.group(3)) * 1000),
                    },
                    "transform": "normal",
                    "power": True,
                    "scale": 1.0,
                    "rect": {"x": 0, "y": 0, "width": width, "height": height},
                }
            ]
        )
    )
elif role == "sunshine":
    values = dict(
        line.split(" = ", 1) for line in Path(arguments[0]).read_text().splitlines()
    )
    login = json.loads(Path(values["credentials_file"]).read_text())
    assert {"username", "salt", "password"} <= login.keys()
    Path(values["cert"]).write_text("-----BEGIN CERTIFICATE-----\nstand-in\n")
    hardware = (knobs / "hardware-nodes").read_text().split()
    kind = "hardware" if values["adapter_name"] in hardware else "software"
    Path(values["log_path"]).write_text((FIXTURES / f"sunshine-startup-{kind}.log").read_text())
    while True:
        time.sleep(1)
else:
    raise SystemExit(2)
