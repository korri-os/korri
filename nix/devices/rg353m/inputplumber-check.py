#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p "python3.withPackages (p: [ p.pyyaml ])"
"""Check RG353M physical mappings after the shared schema validation."""

from pathlib import Path
import sys

import yaml

root = Path(sys.argv[1])


def load(relative):
    return yaml.safe_load((root / relative).read_text())


profile = load("devices/01-rg353m.yaml")
assert profile["matches"] == [
    {
        "udev": {
            "sys_path": "/sys/firmware/devicetree/base",
            "attributes": [
                {"name": "model", "value": "Anbernic"},
                {"name": "compatible", "value": "anbernic,rg353p*"},
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
        "capability_map_id": "rocknix_rg353m_map",
    },
    {
        "group": "keyboard",
        "evdev": {
            "name": "gpio-keys-vol",
            "phys_path": "gpio-keys/input0",
            "handler": "event*",
        },
        "capability_map_id": "anbernic_rg353m_volume",
    },
]
assert profile["maximum_sources"] == 2
# Grounded in the card's rk3566-anbernic-rg353p.dtb (2026-09-30): the
# gpio-keys-vol node sends linux,code 0x73 and 0x72.
volume = load("capability_maps/anbernic_rg353m_volume.yaml")
assert volume["id"] == profile["source_devices"][1]["capability_map_id"]
assert {
    mapping["source_events"][0]["evdev"]["event_code"]: mapping["target_event"]["dbus"]
    for mapping in volume["mapping"]
} == {"KEY_VOLUMEUP": "ui_volume_up", "KEY_VOLUMEDOWN": "ui_volume_down"}
assert volume["filtered_events"] == []
assert profile["options"]["auto_manage"] is True
assert profile["target_devices"] == ["xb360"]
capabilities = load("capability_maps/rocknix_rg353m_map.yaml")
assert capabilities["id"] == profile["source_devices"][0]["capability_map_id"]
# Grounded in the live kernel's gpio-keys-control button-*/linux,code values.
# Native DT uses BTN_NORTH for physical X and BTN_WEST for physical Y.
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
print("RG353M profile and all 16 physical button mappings passed")
