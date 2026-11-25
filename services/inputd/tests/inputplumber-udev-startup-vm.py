#!/usr/bin/env python3
"""InputPlumber starts while udev still processes a USB gamepad.

Tab S6, 2026-11-25: the Kishi re-enumerated while InputPlumber restarted.
Startup discovery cached its evdev node before udevd wrote the database, so
the property-based Kishi config never matched and no composite was made.
This reproduces that order deterministically: a guest-only udev rule holds
the gamepad's event node until the test releases it, after InputPlumber has
already scanned it. Then normal hotplug runs with a delayed udev and no
service restart.
"""

import asyncio
import importlib.util
from pathlib import Path
import subprocess
import sys

spec = importlib.util.spec_from_file_location("reattach", sys.argv[3])
reattach = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reattach)
adoption = reattach.adoption

HOLD = Path("/run/korri-udev-hold")
DELAY = Path("/run/korri-udev-delay")
SOURCES = adoption.ROOT + "/devices/source"


def systemctl(*args):
    subprocess.run(["systemctl", *args], check=True)


def udev_initialized(node):
    # The database entry is what udevd writes when it finishes the rules.
    rdev = Path(node).stat().st_rdev
    return Path(f"/run/udev/data/c{rdev >> 8}:{rdev & 0xff}").exists()


async def source_names(bus):
    try:
        xml = (await adoption.call(bus, SOURCES, "org.freedesktop.DBus.Introspectable",
                                   "Introspect"))[0]
    except AssertionError:
        return set()  # InputPlumber has not claimed its bus name yet
    return {node.attrib["name"] for node in adoption.ET.fromstring(xml).findall("node")}


async def scanned_everything_but(bus, node):
    # Build-independent evidence that startup discovery has run: every other
    # hidraw and evdev node, including the gamepad's own (unheld) hidraw node,
    # has a source object. Unpatched InputPlumber also lists the held node; the
    # fix defers it until udev initializes it.
    held = Path(node).name
    others = {p.name for p in Path("/dev/input").glob("event*")} | \
             {p.name for p in Path("/dev").glob("hidraw*")}
    others.discard(held)
    assert any(name.startswith("hidraw") for name in others), others
    for _ in range(150):
        missing = others - await source_names(bus)
        if not missing:
            # Evdev discovery follows hidraw in the same pass; let it finish.
            await asyncio.sleep(1)
            return
        await asyncio.sleep(0.1)
    raise AssertionError(("InputPlumber never scanned", missing))


async def kishi_composite(bus, node):
    # Exactly one composite, holding exactly this node, with Korri's profile.
    return await adoption.wait_for(bus, lambda state: adoption.sources(state) == {node}
                                   and len(state) == 1
                                   and all(c["ProfilePath"] == adoption.PROFILE
                                           and c["SourceDevicePaths"] == [node]
                                           for c in state.values()))


async def main():
    gadget = reattach.UsbGamepad()
    pad = None
    bus = None
    try:
        # The tablet restarted both providers; the Kishi re-enumerated between
        # their stop and start.
        systemctl("stop", "adoption-profile.service", "inputplumber.service")
        HOLD.touch()
        pad = await gadget.connect()
        node = pad.device_path
        assert not udev_initialized(node), "udev finished before the hold"
        systemctl("start", "inputplumber.service", "adoption-profile.service")
        bus = await adoption.MessageBus(bus_type=adoption.BusType.SYSTEM).connect()
        await scanned_everything_but(bus, node)
        assert not udev_initialized(node), "udev finished before InputPlumber scanned"
        print(f"REPRO: InputPlumber scanned {node} before udev initialized it", flush=True)
        HOLD.unlink()
        try:
            state = await kishi_composite(bus, node)
        except AssertionError:
            # The defect this test reproduces: discovery cached the device
            # before udev initialized it, so its property config never matched.
            print("FAIL-NO-KISHI-COMPOSITE-AFTER-UDEV", flush=True)
            print(subprocess.run(["udevadm", "info", node], capture_output=True,
                                 text=True).stdout, flush=True)
            raise
        path = next(iter(state))
        targets = state[path]["TargetDevices"]
        await asyncio.sleep(0.5)
        await reattach.captured_button(pad)
        print("PASS-STARTUP-CAPTURE", flush=True)

        services = reattach.service_identity()
        DELAY.write_text("1\n")
        for cycle in range(3):
            pad.close()
            pad = None
            await adoption.wait_for(bus, lambda state: not adoption.sources(state))
            pad = await gadget.connect()
            node = pad.device_path
            state = await kishi_composite(bus, node)
            assert list(state) == [path], state
            assert state[path]["TargetDevices"] == targets, state
            await asyncio.sleep(0.5)
            await reattach.captured_button(pad)
            print(f"PASS hotplug cycle={cycle}: delayed udev, same composite, inputd capture",
                  flush=True)
        assert reattach.service_identity() == services, "a provider restarted"
        print("PASS-ALL-NO-RESTART", flush=True)
    finally:
        HOLD.unlink(missing_ok=True)
        DELAY.unlink(missing_ok=True)
        if pad is not None:
            pad.close()
        if bus is not None:
            bus.disconnect()


asyncio.run(main())
