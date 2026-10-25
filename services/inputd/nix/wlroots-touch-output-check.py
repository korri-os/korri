"""Guest-only fixture: real uinput devices, udev properties and compiled wlroots.

Never run this fixture on a host: the NixOS VM check owns its input devices.
No library functions or private implementation details are replaced.
"""
import contextlib
import os
from pathlib import Path
import subprocess
import sys

from evdev import AbsInfo, UInput, ecodes as e

assert os.environ.get("KORRI_TOUCH_TEST_VM") == "1", "requires the isolated NixOS test guest"
axes = {
    e.EV_KEY: [e.BTN_TOUCH],
    e.EV_ABS: [
        (e.ABS_X, AbsInfo(0, 0, 639, 0, 0, 20)),
        (e.ABS_Y, AbsInfo(0, 0, 479, 0, 0, 20)),
        (e.ABS_MT_SLOT, AbsInfo(0, 0, 9, 0, 0, 0)),
        (e.ABS_MT_TRACKING_ID, AbsInfo(0, 0, 65535, 0, 0, 0)),
        (e.ABS_MT_POSITION_X, AbsInfo(0, 0, 639, 0, 0, 20)),
        (e.ABS_MT_POSITION_Y, AbsInfo(0, 0, 479, 0, 0, 20)),
    ],
}
with contextlib.ExitStack() as stack:
    devices = [stack.enter_context(UInput(
        axes, name="Korri Goodix fixture", vendor=1046, product=911,
        phys=f"korri-touch-test/{position}", input_props=[e.INPUT_PROP_DIRECT],
    )) for position in ("top", "bottom", "unmapped")]
    subprocess.run(["udevadm", "settle"], check=True)
    arguments = []
    for device, output in zip(devices, ("DSI-2", "DSI-1", "-")):
        node = device.device.path
        properties = subprocess.check_output(["udevadm", "info", "--query=property", node], text=True).splitlines()
        assert "ID_INPUT_TOUCHSCREEN=1" in properties, properties
        if output != "-":
            assert f"WL_OUTPUT={output}" in properties, properties
        else:
            assert not any(p.startswith("WL_OUTPUT=") for p in properties), properties
        arguments.extend((Path(node).name, output))
    subprocess.run([sys.argv[1], *arguments], check=True, timeout=15)
