"""A steam-arm64-seed fixture: records what the seed sees, then installs a client."""
import json
import os
from pathlib import Path

home = Path(os.environ["STEAM_HOME"])
cache = Path(os.environ["KORRI_CACHE_ROOT"])
(home / "seed.json").write_text(json.dumps({
    "rootfs": (cache / "fex/rootfs/fixture").is_dir(),
    "proton": (cache / "proton/cachyos/fixture").is_dir(),
    "STEAM_PROTON_DIR": os.environ.get("STEAM_PROTON_DIR"),
}))
client = home / "steamrtarm64/steam"
client.parent.mkdir(parents=True, exist_ok=True)
client.touch()
client.chmod(0o755)
