"""Local command provider for exercising the Nix transport invocation, not a device."""

import json
import os
from pathlib import Path
import sys

with Path(os.environ["CADENCE_TRANSPORT_LOG"]).open("a") as log:
    log.write(
        json.dumps(
            {"arguments": sys.argv[1:], "ssh_options": os.environ.get("NIX_SSHOPTS")}
        )
        + "\n"
    )
if sys.argv[1] == "build":
    print(
        json.dumps(
            [
                {"outputs": {"out": "/nix/store/transport-python3-1.0"}},
                {"outputs": {"out": "/nix/store/transport-stream-cadence-motion"}},
            ]
        )
    )
elif sys.argv[1] == "eval":
    print("/nix/store/transport-source")
