"""A line-protocol peer with a delayed sample, for interruption checks."""

import json
import sys
import time

for index, line in enumerate(sys.stdin, 1):
    request = json.loads(line)
    if request["operation"] == "sample":
        time.sleep(0.15)
        value = {"threads": [], "journal": "sample completed"}
    elif request["operation"] == "discover":
        value = {"display": {"refresh_hz": 60}, "working_encoders": False}
    else:
        value = {"pass": True}
    print(json.dumps({"request_id": index, "ok": value}), flush=True)
    if request["operation"] == "close":
        break
