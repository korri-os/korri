"""Stand-ins for the two programs that a host-mode viewer fixture starts.

As a compositor (`--config FILE`) it opens the Wayland socket and waits. As a
viewer (`pair -keydir DIR ...`) it prints a PIN the way Moonlight does, waits
until DIR holds `admitted` or `refused`, and exits like Moonlight.
"""

import os
from pathlib import Path
import signal
import socket
import sys
import time


def stop(_signum: int, _frame: object) -> None:
    raise SystemExit(0)


signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
if sys.argv[1] == "--config":
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as display:
        display.bind(str(Path(os.environ["XDG_RUNTIME_DIR"]) / "wayland-1"))
        display.listen(1)
        while True:
            time.sleep(1)
elif sys.argv[1] == "pair":
    keys = Path(sys.argv[sys.argv.index("-keydir") + 1])
    print("Please enter the following PIN on the target PC: 4321", flush=True)
    while True:
        if (keys / "admitted").exists():
            print("Succesfully paired", flush=True)
            raise SystemExit(0)
        if (keys / "refused").exists():
            print("Failed to pair to server: Pin is incorrect", flush=True)
            raise SystemExit(1)
        time.sleep(0.05)
raise SystemExit(2)
