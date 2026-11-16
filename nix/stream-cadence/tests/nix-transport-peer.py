"""Local command provider for exercising the Nix transport invocation, not a device.

Installed as `nix` and as `nix-store`. Its knobs:

- CADENCE_TRANSPORT_BUILD=plan: `nix build --dry-run` plans a build and two
  fetches, as Nix words them, and the build then fails as Nix does when it
  may not build on this host. CADENCE_TRANSPORT_BUILD=refuse: the plan shows
  no build, and the build fails all the same.
- CADENCE_TRANSPORT_HOLDS=STORE: the store that holds every path that
  `nix-store --check-validity` asks about (`auto` is this host's store). Every
  other store holds none of them.
- CADENCE_TRANSPORT_COPY_FROM=refuse: `nix copy --from` fails as Nix does for a
  path that no key of this host signed.
"""

import json
import os
from pathlib import Path
import sys

program = Path(sys.argv[0]).name
arguments = sys.argv[1:]
with Path(os.environ["CADENCE_TRANSPORT_LOG"]).open("a") as log:
    log.write(
        json.dumps(
            {
                "program": program,
                "arguments": arguments,
                "ssh_options": os.environ.get("NIX_SSHOPTS"),
            }
        )
        + "\n"
    )
if program == "nix-store":
    if "--check-validity" in arguments:
        store = arguments[arguments.index("--store") + 1] if "--store" in arguments else "auto"
        if store != os.environ.get("CADENCE_TRANSPORT_HOLDS"):
            for path in arguments:
                if path.startswith("/nix/store/"):
                    print(path)
elif arguments[0] == "build" and "--dry-run" in arguments:
    if os.environ.get("CADENCE_TRANSPORT_BUILD") == "plan":
        print(
            "this derivation will be built:\n"
            "  /nix/store/transport-stream-cadence-motion.drv\n"
            "these 2 paths will be fetched (1.0 MiB download, 3.3 MiB unpacked):\n"
            "  /nix/store/transport-wayland-1.0\n"
            "  /nix/store/transport-weston-dev-1.0",
            file=sys.stderr,
        )
elif arguments[0] == "build":
    if os.environ.get("CADENCE_TRANSPORT_BUILD") in ("plan", "refuse"):
        print(
            "error: Cannot build '/nix/store/transport-stream-cadence-motion.drv'.\n"
            "       Reason: local builds are disabled (max-jobs = 0)",
            file=sys.stderr,
        )
        sys.exit(1)
    print(
        json.dumps(
            [
                {"outputs": {"out": "/nix/store/transport-python3-1.0"}},
                {"outputs": {"out": "/nix/store/transport-stream-cadence-motion"}},
            ]
        )
    )
elif arguments[0] == "eval":
    # As `nix eval --raw` writes it: no newline.
    sys.stdout.write("/nix/store/transport-source")
elif arguments[0] == "copy" and "--from" in arguments:
    if os.environ.get("CADENCE_TRANSPORT_COPY_FROM") == "refuse":
        print(
            "error: cannot add path '/nix/store/transport-stream-cadence-motion' "
            "because it lacks a signature by a trusted key",
            file=sys.stderr,
        )
        sys.exit(1)
