"""A viewer agent's line protocol for one recording: parts of it, then its removal.

The real agent (viewer.py) answers the same operations from its run directory.
"""

import base64
import json
from pathlib import Path
import sys
import time

recording = Path(sys.argv[1])
for line in sys.stdin:
    request = json.loads(line)
    operation = request["operation"]
    if operation == "recording":
        with recording.open("rb") as stream:
            stream.seek(request["offset"])
            value: dict = {"data": base64.b64encode(stream.read(request["length"])).decode()}
    elif operation == "discard_recording":
        recording.unlink(missing_ok=True)
        value = {}
    elif operation == "clock":
        value = {"device_ns": time.monotonic_ns()}
    else:
        value = {"pass": True}
    print(json.dumps({"request_id": request["request_id"], "ok": value}), flush=True)
    if operation == "close":
        break
