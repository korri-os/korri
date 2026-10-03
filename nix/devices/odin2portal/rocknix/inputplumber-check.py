#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p "python3.withPackages (p: [ p.pyyaml ])"
"""Check the Portal's ROCKNIX map and Korri input topology against live hardware."""

from pathlib import Path
import sys

import yaml

root = Path(sys.argv[1])
profile = yaml.safe_load((root / "devices/02-ayn-controller.yaml").read_text())
assert profile["target_devices"] == ["xb360"], "inputd captures XB360, not DualSense"
assert profile["maximum_sources"] == 3
assert profile["matches"] == [
    {
        "udev": {
            "sys_path": "/sys/firmware/devicetree/base",
            "attributes": [{"name": "model", "value": "AYN Odin 2 Portal"}],
        }
    }
]
assert profile["options"]["auto_manage"] is True
assert profile["source_devices"] == [
    {
        "group": "gamepad",
        "evdev": {
            "name": "AYN Odin2 Gamepad",
            "phys_path": "rsinput-gamepad/input0",
            "handler": "event*",
        },
        "capability_map_id": "ayn_mcu",
    },
    {
        "group": "keyboard",
        "evdev": {
            "name": "gpio-keys",
            "phys_path": "gpio-keys/input0",
            "handler": "event*",
        },
        "capability_map_id": "ayn_odin2portal_volume",
    },
    {
        "group": "keyboard",
        "evdev": {
            "name": "pmic_resin",
            "phys_path": "pmic_resin/input0",
            "handler": "event*",
        },
        "capability_map_id": "ayn_odin2portal_volume",
    },
]

# Keep the vendored ROCKNIX AYN MCU map, including its X/Y swap and Z/RZ
# triggers. Unlike the Mini V2, the Portal exposes no HAT2X/HAT2Y axes.
cap = yaml.safe_load((root / "capability_maps/ayn_mcu.yaml").read_text())
assert cap["id"] == "ayn_mcu"
buttons, triggers, axes, keyboard = {}, {}, {}, {}
for mapping in cap["mapping"]:
    sources = [event["evdev"] for event in mapping["source_events"]]
    codes = tuple(source["event_code"] for source in sources)
    target = mapping["target_event"]
    if "keyboard" in target:
        assert len(sources) == 1
        keyboard[codes[0]] = target["keyboard"]
        continue
    target = target["gamepad"]
    if "button" in target:
        assert len(sources) == 1
        assert sources[0]["event_type"] == "KEY"
        assert sources[0]["value_type"] == "button"
        buttons[codes[0]] = target["button"]
    elif "trigger" in target:
        assert len(sources) == 1
        assert sources[0]["event_type"] == "ABS"
        assert sources[0]["value_type"] == "trigger"
        triggers[codes[0]] = target["trigger"]["name"]
    else:
        assert [s["event_type"] for s in sources] == ["ABS", "ABS"]
        assert [s["value_type"] for s in sources] == ["joystick_x", "joystick_y"]
        axes[codes] = target["axis"]["name"]
assert buttons == {
    "BTN_MODE": "Guide",
    "BTN_SOUTH": "South",
    "BTN_EAST": "East",
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
    "BTN_Z": "LeftPaddle2",
    "BTN_C": "RightPaddle2",
}
assert triggers == {"ABS_Z": "LeftTrigger", "ABS_RZ": "RightTrigger"}
assert axes == {("ABS_X", "ABS_Y"): "LeftStick", ("ABS_RX", "ABS_RY"): "RightStick"}
# Preserve the original map as reference, but do not create an unowned keyboard
# target. As on Mini V2, F1 has no Korri action in this slice. The Portal's live
# bitmap exposes BTN_BACK, but not KEY_F24 or the Odin 2's paddle keys.
assert keyboard == {"BTN_BACK": "KeyF1", "KEY_F24": "KeyHome"}
assert cap["filtered_events"] == []

volume = yaml.safe_load(
    (root / "capability_maps/ayn_odin2portal_volume.yaml").read_text()
)
assert volume["id"] == "ayn_odin2portal_volume"
actual = {}
for mapping in volume["mapping"]:
    assert len(mapping["source_events"]) == 1
    source = mapping["source_events"][0]["evdev"]
    assert source["event_type"] == "KEY"
    assert source["value_type"] == "button"
    actual[source["event_code"]] = mapping["target_event"]
assert actual == {
    "KEY_VOLUMEUP": {"dbus": "ui_volume_up"},
    "KEY_VOLUMEDOWN": {"dbus": "ui_volume_down"},
}
assert volume["filtered_events"] == []
print(
    "Portal: ROCKNIX MCU map preserved; XB360 target and both volume sources verified"
)
