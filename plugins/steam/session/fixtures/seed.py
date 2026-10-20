"""A steam-arm64-seed fixture: records what the seed sees, then installs a client."""
import json
import os
from pathlib import Path

home = Path(os.environ["STEAM_HOME"])
(home / "seed.json").write_text(json.dumps({"FEX_ROOTFS": os.environ.get("FEX_ROOTFS")}))
client = home / "steamrtarm64/steam"
client.parent.mkdir(parents=True, exist_ok=True)
client.touch()
client.chmod(0o755)
