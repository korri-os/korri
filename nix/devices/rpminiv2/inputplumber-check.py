#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p "python3.withPackages (p: [ p.pyyaml ])"
"""Check the RP Mini V2 profile against its kernel gamepad contract."""

from pathlib import Path
import sys

import yaml

root = Path(sys.argv[1])


def load(relative):
    return yaml.safe_load((root / relative).read_text())


profile = load("devices/01-retroid-pocket-mini-v2.yaml")
# The live device reports this DMI product name to InputPlumber. A match
# against /sys/firmware/devicetree/base did not create a composite on hardware.
assert profile["matches"] == [{"dmi_data": {"product_name": "Retroid Pocket Mini V2"}}]
assert profile["source_devices"] == [
    {
        "group": "gamepad",
        "evdev": {
            "name": "Retroid Pocket Gamepad",
            "phys_path": "retroid-pocket-gamepad/input0",
            "handler": "event*",
        },
        "capability_map_id": "retroid_pocket_mini_v2",
    }
]
assert profile["maximum_sources"] == 1
assert profile["options"]["auto_manage"] is True
assert profile["target_devices"] == ["xb360"]

capabilities = load("capability_maps/retroid_pocket_mini_v2.yaml")
assert capabilities["id"] == profile["source_devices"][0]["capability_map_id"]

expected_buttons = {
    "BTN_MODE": "Guide",
    "BTN_SOUTH": "South",
    "BTN_EAST": "East",
    # ROCKNIX's Retroid MCU map confirms these two legacy Linux codes need
    # swapping to preserve physical X=West and Y=North on a standard pad.
    "BTN_NORTH": "West",
    "BTN_WEST": "North",
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
}
# The vendor driver also exposes BTN_BACK. ROCKNIX routes it to a separate F1
# keyboard target. This first milestone intentionally creates only one xb360
# target so an unowned F1 shortcut cannot escape into Chromium.
kernel_buttons = set(expected_buttons) | {"BTN_BACK"}
intentionally_unbound_buttons = {"BTN_BACK"}

expected_triggers = {
    "ABS_HAT2X": "LeftTrigger",
    "ABS_HAT2Y": "RightTrigger",
}
expected_axes = {
    ("ABS_X", "ABS_Y"): "LeftStick",
    ("ABS_RX", "ABS_RY"): "RightStick",
}

buttons = {}
triggers = {}
axes = {}
for mapping in capabilities["mapping"]:
    sources = [event["evdev"] for event in mapping["source_events"]]
    target = mapping["target_event"]["gamepad"]
    if "button" in target:
        assert len(sources) == 1
        source = sources[0]
        assert source["event_type"] == "KEY"
        assert source["value_type"] == "button"
        buttons[source["event_code"]] = target["button"]
    elif "trigger" in target:
        assert len(sources) == 1
        source = sources[0]
        assert source["event_type"] == "ABS"
        assert source["value_type"] == "trigger"
        triggers[source["event_code"]] = target["trigger"]["name"]
    else:
        assert len(sources) == 2
        assert [source["event_type"] for source in sources] == ["ABS", "ABS"]
        assert [source["value_type"] for source in sources] == [
            "joystick_x",
            "joystick_y",
        ]
        axes[tuple(source["event_code"] for source in sources)] = target["axis"]["name"]

assert set(buttons) == kernel_buttons - intentionally_unbound_buttons, buttons
assert buttons == expected_buttons, buttons
# The driver reports analog triggers on HAT2X/HAT2Y, not its unused Z/RZ axes.
assert triggers == expected_triggers, triggers
assert axes == expected_axes, axes
assert capabilities["filtered_events"] == []
print(
    "RP Mini V2 profile, 15 mapped buttons, 1 intentionally unbound button, "
    "2 triggers, and 2 sticks passed"
)
