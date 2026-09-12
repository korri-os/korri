#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p "python3.withPackages (p: [ p.pyyaml ])"
"""Check RG DS physical mappings after the shared schema validation."""

from pathlib import Path
import sys

import yaml

root = Path(sys.argv[1])


def load(relative):
    return yaml.safe_load((root / relative).read_text())


profile = load("devices/01-rgds.yaml")
# Read from the running device: /proc/device-tree/model and the evdev name
# and physical path of its GPIO gamepad.
assert profile["matches"] == [
    {
        "udev": {
            "sys_path": "/sys/firmware/devicetree/base",
            "attributes": [
                {"name": "model", "value": "Anbernic RG DS"},
                {"name": "compatible", "value": "anbernic,rg-ds*"},
            ],
        }
    }
]
assert profile["source_devices"] == [
    {
        "group": "gamepad",
        "evdev": {
            "name": "gpio-keys-control",
            "phys_path": "gpio-keys/input0",
            "handler": "event*",
        },
        "capability_map_id": "rgds_map",
    }
]
assert profile["maximum_sources"] == 1
assert profile["options"]["auto_manage"] is True
assert profile["target_devices"] == ["xb360"]
capabilities = load("capability_maps/rgds_map.yaml")
assert capabilities["id"] == profile["source_devices"][0]["capability_map_id"]
# Every code below appears in rk3568-anbernic-rg-ds.dts under gpio-keys-control.
# The lid switch, home key, and analog stick belong to other devices and are
# deliberately absent from this button map.
expected = {
    "BTN_SOUTH": "South",
    "BTN_EAST": "East",
    "BTN_NORTH": "North",
    "BTN_WEST": "West",
    "BTN_START": "Start",
    "BTN_SELECT": "Select",
    "BTN_TL": "LeftBumper",
    "BTN_TR": "RightBumper",
    "BTN_THUMBL": "LeftStick",
    "BTN_THUMBR": "RightStick",
    "BTN_DPAD_UP": "DPadUp",
    "BTN_DPAD_DOWN": "DPadDown",
    "BTN_DPAD_LEFT": "DPadLeft",
    "BTN_DPAD_RIGHT": "DPadRight",
    "BTN_TL2": "LeftTrigger",
    "BTN_TR2": "RightTrigger",
}
actual = {}
for mapping in capabilities["mapping"]:
    assert len(mapping["source_events"]) == 1
    source = mapping["source_events"][0]["evdev"]
    assert source["event_type"] == "KEY"
    code = source["event_code"]
    assert code not in actual
    target = mapping["target_event"]["gamepad"]
    actual[code] = target.get("button") or target["trigger"]["name"]
assert actual == expected, actual
print("RG DS profile and all 16 physical button mappings passed")
