#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p "python3.withPackages (p: [ p.pyyaml ])"
"""Check the Anbernic H700 mappings after the shared schema validation.

The expected values restate the board trees (sun50i-h700-anbernic-rg35xx-*.dts
after kernel patches 0211 and 0223) and the pinned rocknix-singleadc-joypad
source. A change to any of them must change this check too.
"""

from pathlib import Path
import sys

import yaml

root = Path(sys.argv[1])


def load(relative):
    return yaml.safe_load((root / relative).read_text())


profile = load("devices/01-anbernic-h700.yaml")
assert profile["matches"] == [
    {
        "udev": {
            "sys_path": "/sys/firmware/devicetree/base",
            "attributes": [
                {"name": "model", "value": model},
                {"name": "compatible", "value": compatible},
            ],
        }
    }
    for model, compatible in [
        ("Anbernic RG35XX SP", "anbernic,rg35xx-sp*"),
        ("Anbernic RG35XX Pro", "anbernic,rg35xx-pro*"),
    ]
]
assert profile["source_devices"] == [
    {
        "group": "gamepad",
        "evdev": {
            "name": "H700 Gamepad",
            "phys_path": "rocknix-singleadc-joypad/input0",
            "handler": "event*",
        },
        "capability_map_id": "anbernic_h700_gamepad",
    },
    {
        "group": "keyboard",
        "evdev": {
            "name": "gpio-keys-volume",
            "phys_path": "gpio-keys/input0",
            "handler": "event*",
        },
        "capability_map_id": "anbernic_h700_volume",
    },
]
assert profile["maximum_sources"] == 2
assert profile["options"]["auto_manage"] is True
assert profile["target_devices"] == ["xb360"]

gamepad = load("capability_maps/anbernic_h700_gamepad.yaml")
assert gamepad["id"] == profile["source_devices"][0]["capability_map_id"]
buttons = {
    mapping["source_events"][0]["evdev"]["event_code"]: mapping["target_event"]["gamepad"]
    for mapping in gamepad["mapping"]
    if len(mapping["source_events"]) == 1
}
# Every joypad sw* linux,code in the 2024 board tree, by position.
assert buttons == {
    "BTN_MODE": {"button": "Guide"},
    "BTN_SOUTH": {"button": "South"},
    "BTN_EAST": {"button": "East"},
    "BTN_NORTH": {"button": "North"},
    "BTN_WEST": {"button": "West"},
    "BTN_START": {"button": "Start"},
    "BTN_SELECT": {"button": "Select"},
    "BTN_TL": {"button": "LeftBumper"},
    "BTN_TR": {"button": "RightBumper"},
    "BTN_TL2": {"trigger": {"name": "LeftTrigger"}},
    "BTN_TR2": {"trigger": {"name": "RightTrigger"}},
    "BTN_THUMBL": {"button": "LeftStick"},
    "BTN_THUMBR": {"button": "RightStick"},
    "BTN_DPAD_UP": {"button": "DPadUp"},
    "BTN_DPAD_DOWN": {"button": "DPadDown"},
    "BTN_DPAD_LEFT": {"button": "DPadLeft"},
    "BTN_DPAD_RIGHT": {"button": "DPadRight"},
}
sticks = {
    tuple(event["evdev"]["event_code"] for event in mapping["source_events"]): mapping[
        "target_event"
    ]["gamepad"]["axis"]["name"]
    for mapping in gamepad["mapping"]
    if len(mapping["source_events"]) == 2
}
# The Pro's amux-channel-mapping: ABS_RY ABS_RX ABS_Y ABS_X.
assert sticks == {
    ("ABS_X", "ABS_Y"): "LeftStick",
    ("ABS_RX", "ABS_RY"): "RightStick",
}
assert gamepad["filtered_events"] == []

volume = load("capability_maps/anbernic_h700_volume.yaml")
assert volume["id"] == profile["source_devices"][1]["capability_map_id"]
assert {
    mapping["source_events"][0]["evdev"]["event_code"]: mapping["target_event"]["dbus"]
    for mapping in volume["mapping"]
} == {"KEY_VOLUMEUP": "ui_volume_up", "KEY_VOLUMEDOWN": "ui_volume_down"}
assert volume["filtered_events"] == []
