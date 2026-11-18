"""Check the Thor composite against the Portal's shared maps."""

from pathlib import Path
import sys

import yaml

root = Path(sys.argv[1])
profile = yaml.safe_load((root / "devices/02-ayn-thor-controller.yaml").read_text())
assert profile["target_devices"] == ["xb360"], "inputd captures XB360, not DualSense"
assert profile["matches"] == [
    {
        "udev": {
            "sys_path": "/sys/firmware/devicetree/base",
            "attributes": [{"name": "model", "value": "AYN Thor"}],
        }
    }
]
assert profile["options"]["auto_manage"] is True
sources = profile["source_devices"]
assert profile["maximum_sources"] == len(sources)
assert [s["evdev"]["phys_path"] for s in sources] == [
    "rsinput-gamepad/input0",
    "gpio-keys/input0",
    "pmic_resin/input0",
]
# The AYN key (KEY_F24, gpio-keys-ayn) stays unbound in the first slice.
assert all("gpio-keys-ayn" not in str(s) for s in sources)
maps = {p.stem: yaml.safe_load(p.read_text()) for p in (root / "capability_maps").glob("*.yaml")}
ids = {m["id"] for m in maps.values()}
assert {s["capability_map_id"] for s in sources} <= ids
# Only the Thor composite: the Portal's must not match here too.
assert sorted(p.name for p in (root / "devices").iterdir()) == ["02-ayn-thor-controller.yaml"]
print("Thor: XB360 target, MCU and volume sources, AYN key unbound")
