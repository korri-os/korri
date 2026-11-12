#!/usr/bin/env python3
"""Kernel/DBus adoption matrix; run only in the dedicated NixOS guest."""

import asyncio
import json
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET

NAME = "korri-inputplumber-adoption"
if (
    Path("/etc/hostname").read_text().strip() != NAME
    or Path(f"/etc/{NAME}-vm").read_text() != "isolated-kernel-proof-only\n"
    or Path("/sys/class/dmi/id/product_name").read_text().strip() != NAME
):
    raise SystemExit("refusing adoption proof outside its dedicated NixOS VM")

from dbus_next import BusType, Message, MessageType
from dbus_next.aio import MessageBus
from evdev import AbsInfo, UInput, ecodes as e

SERVICE = "org.shadowblip.InputPlumber"
ROOT = "/org/shadowblip/InputPlumber"
COMPOSITE = "org.shadowblip.Input.CompositeDevice"
PROFILE = sys.argv[1]


def device(name, vendor, product, version=0x0101, phys=None):
    axes = {
        code: AbsInfo(0, -32768, 32767, 0, 0, 0)
        for code in (e.ABS_X, e.ABS_Y, e.ABS_RX, e.ABS_RY)
    }
    axes.update({code: AbsInfo(0, 0, 255, 0, 0, 0)
                 for code in (e.ABS_Z, e.ABS_RZ)})
    axes.update({code: AbsInfo(0, -1, 1, 0, 0, 0)
                 for code in (e.ABS_HAT0X, e.ABS_HAT0Y)})
    return UInput({
        e.EV_KEY: [e.BTN_SOUTH, e.BTN_EAST, e.BTN_NORTH, e.BTN_WEST,
                   e.BTN_TL, e.BTN_TR, e.BTN_SELECT, e.BTN_START,
                   e.BTN_MODE, e.BTN_THUMBL, e.BTN_THUMBR],
        e.EV_ABS: list(axes.items()),
    }, name=name, vendor=vendor, product=product, version=version,
        bustype=e.BUS_USB, phys=phys or f"adoption-proof/{vendor:04x}:{product:04x}/{name}")


async def call(bus, path, interface, member, signature="", body=None):
    reply = await asyncio.wait_for(bus.call(Message(
        destination=SERVICE, path=path, interface=interface, member=member,
        signature=signature, body=body or [],
    )), 15)
    assert reply.message_type == MessageType.METHOD_RETURN, (
        path, member, reply.error_name, reply.body)
    return reply.body


async def snapshot(bus):
    xml = (await call(bus, ROOT, "org.freedesktop.DBus.Introspectable", "Introspect"))[0]
    result = {}
    for node in ET.fromstring(xml).findall("node"):
        name = node.attrib.get("name", "")
        if not name.startswith("CompositeDevice") or not name[15:].isdigit():
            continue
        path = f"{ROOT}/{name}"
        # Read live properties using the same contract as inputd, not bulk
        # ObjectManager serialization of protected getters.
        properties = {}
        for key in ("SourceDevicePaths", "ProfilePath", "TargetDevices", "DbusDevices"):
            properties[key] = (await call(
                bus, path, "org.freedesktop.DBus.Properties", "Get", "ss",
                [COMPOSITE, key]))[0].value
        result[path] = properties
    return result


async def wait_for(bus, predicate):
    last = None
    for _ in range(150):
        last = await snapshot(bus)
        if predicate(last):
            return last
        await asyncio.sleep(0.2)
    raise AssertionError(last)


def sources(snapshot):
    return {node for composite in snapshot.values()
            for node in composite["SourceDevicePaths"]}


async def main():
    bus = await MessageBus(bus_type=BusType.SYSTEM).connect()
    manager = ROOT + "/Manager"
    manage_all = (await call(bus, manager, "org.freedesktop.DBus.Properties",
                            "Get", "ss", ["org.shadowblip.InputManager",
                                            "ManageAllDevices"]))[0].value
    assert manage_all is False
    outputs = [device(f"Korri Seat P{slot}", 0x045e, 0x028e, 0x0114,
                      f"korri/input-seat/p{slot}") for slot in range(1, 5)]
    outputs += [device("Microsoft X-Box 360 pad", 0x045e, 0x028e, 1)]
    # Give even the keyboard/mice joystick capabilities: a stronger regression
    # than relying on udev not classifying them as joysticks on this kernel.
    outputs += [device(name, 0xbeef, 0xdead) for name in (
        "libvirtualhid Keyboard", "libvirtualhid Mouse",
        "libvirtualhid Mouse (Absolute)")]
    deferred = [device(f"Sony Interactive Entertainment Wireless Controller{suffix}",
                       0x054c, product)
                for product in (0x0ce6, 0x0df2, 0x09cc)
                for suffix in ("", " Touchpad", " Motion Sensors")]
    pads = []
    try:
        subprocess.run(["udevadm", "settle"], check=True)
        for handle in outputs + deferred:
            properties = subprocess.check_output([
                "udevadm", "info", "--query=property",
                f"--path=/sys/class/input/{Path(handle.device.path).name}",
            ], text=True).splitlines()
            assert "ID_BUS=bluetooth" in properties, properties
            assert "ID_INPUT_JOYSTICK=1" in properties, properties
        await asyncio.sleep(2)
        assert not sources(await snapshot(bus)), await snapshot(bus)
        models = [
            ("Microsoft X-Box 360 pad", 0x045e, 0x028e),
            ("Microsoft X-Box One pad", 0x045e, 0x02ea),
            ("Microsoft X-Box One Elite pad", 0x045e, 0x02e3),
            ("Microsoft X-Box pad", 0x045e, 0x0202),
            ("Logitech Dual Action", 0x046d, 0xc216),
            ("Nintendo Switch Pro Controller", 0x057e, 0x2009),
            ("8BitDo Pro 2", 0x2dc8, 0x6006),
            ("Unknown generic joystick", 0x1234, 0xabcd),
            ("Second wired Xbox 360", 0x045e, 0x028e),
        ]
        for name, vendor, product in models:
            pad = device(name, vendor, product)
            pads.append(pad)
            node = pad.device.path
            subprocess.run(["udevadm", "settle"], check=True)
            # Regression: on main, the first wired 360 never gets a composite.
            state = await wait_for(bus, lambda state: node in sources(state)
                                   and all(c["ProfilePath"] == PROFILE
                                           and len(c["TargetDevices"]) == 3
                                           and len(c["DbusDevices"]) == 1
                                           for c in state.values()
                                           if c["SourceDevicePaths"]))
            composite = [c for c in state.values()
                         if node in c["SourceDevicePaths"]]
            assert len(composite) == 1 and composite[0]["SourceDevicePaths"] == [node]
            print(json.dumps({"model": name, "node": node, "composite": composite}))
        # All nine simultaneously, including two wired 360 pads, each on its
        # own composite. AmbiguousSources is a per-composite guard, not a
        # one-controller product limit.
        state = await snapshot(bus)
        expected = {pad.device.path for pad in pads}
        assert sources(state) == expected, state
        assert len([c for c in state.values() if c["SourceDevicePaths"]]) == len(pads)
        for composite in state.values():
            if not composite["SourceDevicePaths"]:
                continue
            assert composite["ProfilePath"] == PROFILE, composite
            target_types = []
            assert len(composite["DbusDevices"]) == 1, composite
            for target in composite["TargetDevices"]:
                target_types.append((await call(
                    bus, target, "org.freedesktop.DBus.Properties", "Get", "ss",
                    ["org.shadowblip.Input.Target", "DeviceType"]))[0].value)
            assert sorted(target_types) == ["keyboard", "mouse", "xb360"], target_types
        # New output while pads are managed must not attach to any composite.
        late_output = device("Korri Seat P5", 0x045e, 0x028e, 0x0114,
                             "korri/input-seat/p5")
        outputs.append(late_output)
        subprocess.run(["udevadm", "settle"], check=True)
        await asyncio.sleep(2)
        assert sources(await snapshot(bus)) == expected
        # Persistence leaves an empty source slot. New devices must still pass
        # the ordered ignore policy before the old matcher's reconnect path.
        failures = []
        for vacancy in (7, 0):  # generic fallback, then Xbox 360-specific matcher
            pad = pads[vacancy]
            node = pad.device.path
            before = await snapshot(bus)
            path = next(path for path, c in before.items()
                        if node in c["SourceDevicePaths"])
            targets = before[path]["TargetDevices"]
            pad.close()
            expected.remove(node)
            state = await wait_for(bus, lambda state: sources(state) == expected)
            assert path in state and state[path]["SourceDevicePaths"] == [], state
            assert state[path]["TargetDevices"] == targets, state
            excluded_models = [
                ("Microsoft X-Box 360 pad", 0x045e, 0x028e, 1, None),
                ("Korri Seat P1", 0x045e, 0x028e, 0x0114, "korri/input-seat/p1"),
                ("Sony Interactive Entertainment Wireless Controller",
                 0x054c, 0x0ce6, 0x0101, None),
            ]
            for model in excluded_models:
                excluded = device(*model)
                try:
                    subprocess.run(["udevadm", "settle"], check=True)
                    properties = subprocess.check_output([
                        "udevadm", "info", "--query=property",
                        f"--path=/sys/class/input/{Path(excluded.device.path).name}",
                    ], text=True).splitlines()
                    assert "ID_BUS=bluetooth" in properties, properties
                    assert "ID_INPUT_JOYSTICK=1" in properties, properties
                    # Check repeatedly, not just before the add event arrives.
                    for _ in range(15):
                        await asyncio.sleep(0.2)
                        state = await snapshot(bus)
                        if sources(state) != expected:
                            failures.append({"vacancy": models[vacancy][0],
                                             "excluded": model[0], "state": state})
                            break
                    print(json.dumps({"vacancy": models[vacancy][0],
                                      "excluded": model[0], "state": state}))
                finally:
                    excluded.close()
                await wait_for(bus, lambda state: sources(state) == expected)
            # A real reconnect must still reuse the persistent composite/targets.
            pads[vacancy] = device(*models[vacancy])
            node = pads[vacancy].device.path
            expected.add(node)
            state = await wait_for(bus, lambda state: sources(state) == expected)
            assert state[path]["SourceDevicePaths"] == [node], state
            assert state[path]["TargetDevices"] == targets, state
        assert not failures, json.dumps(failures)
        print("PASS: nine simultaneous pads carry the Korri profile; outputs and Sony pads ignored before and after disconnect; generic and Xbox 360 composites reconnect with unchanged targets; ManageAllDevices=false")
    finally:
        for handle in pads + outputs + deferred:
            handle.close()
        bus.disconnect()


asyncio.run(main())
