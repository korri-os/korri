"""A subprocess fixture implementing client IPC and lifecycle log events."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time

home = Path(os.environ["STEAM_HOME"])
log = home / "logs/console_log.txt"
behavior = os.environ.get("CLIENT_BEHAVIOR", "game")


def event(line):
    with log.open("a") as stream:
        stream.write(line + "\n")


if "-shutdown" in sys.argv:
    (home / "shutdown").touch()
    sys.exit(0)
if "-applaunch" in sys.argv:
    count = home / "forwards"
    forwards = int(count.read_text()) + 1 if count.exists() else 1
    count.write_text(str(forwards))
    if behavior == "forward-error":
        sys.exit(1)
    if behavior in ("forward-restart", "forward-client-restart", "forward-restart-always") and (
            forwards == 1 or behavior == "forward-restart-always"):
        restart = home / "restart"
        restart.touch()
        if behavior == "forward-client-restart":
            # The forwarding error is not 42, but the managed client has
            # independently exited 42 before this invocation returns.
            deadline = time.monotonic() + 5
            while restart.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            time.sleep(0.1)
            sys.exit(1)
        sys.exit(42)
    if behavior == "update" and not (home / "update-complete").exists():
        sys.exit(70)
    if behavior == "drop-first" and forwards == 1:
        # Steam accepted the IPC but acted on nothing, as before sign-in.
        sys.exit(0)
    (home / ("restart" if behavior == "restart" and forwards == 1 else "launch")).touch()
    sys.exit(0)

(home / "shutdown").unlink(missing_ok=True)
manifest = home / "package/steam_client_steamdeck_stable_linuxarm64.manifest"
with (home / "manifest-seen").open("a") as stream:
    stream.write(manifest.read_text() if manifest.exists() else "missing\n")
beta = home / "package/beta"
(home / "beta-seen").write_text(beta.read_text() if beta.exists() else "missing\n")
(home / "client-env.json").write_text(json.dumps(
    {name: os.environ.get(name) for name in ("FEX_ROOTFS", "FEX_APP_CONFIG_LOCATION")}))
if behavior == "update":
    event("Found pending update")
    event("Waiting for compat in post-logon")
    time.sleep(2)
    (home / "update-complete").touch()
    event("Update complete, launching Steam...")
event("Waiting for compat in post-logon")
while not (home / "shutdown").exists():
    if (home / "restart").exists():
        (home / "restart").unlink()
        sys.exit(42)
    if (home / "launch").exists():
        (home / "launch").unlink()
        event("GameAction [AppID 401710, ActionID 1] : LaunchApp changed task to Completed")
        if manifest.exists():
            # The client clears its channel and this file during a session.
            manifest.unlink()
        game = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(2)"],
            env={**os.environ, "SteamAppId": "401710"},
        )
        event("Game process added : AppID 401710")
        # Removal alone is insufficient: the session must wait for the real
        # process with SteamAppId to exit as well.
        event("Game process removed: AppID 401710")
        game.wait()
    time.sleep(0.05)
