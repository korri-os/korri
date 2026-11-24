#!/usr/bin/env python3
"""Real kernel removal, devnode replay and inputd capture, without restarts."""

import asyncio
import importlib.util
import json
import os
import stat
from pathlib import Path
import struct
import subprocess
import sys

spec = importlib.util.spec_from_file_location("adoption", sys.argv[2])
adoption = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adoption)


class UsbGamepad:
    """Native configfs HID gadget on the guest's dummy USB host controller."""

    def __init__(self):
        self.root = Path("/sys/kernel/config/usb_gadget/reattach")
        self.root.mkdir()
        self.put("idVendor", "0x1532")
        self.put("idProduct", "0x0712")
        self.put("bcdUSB", "0x0200")
        (self.root / "strings/0x409").mkdir()
        self.put("strings/0x409/manufacturer", "Razer")
        self.put("strings/0x409/product", "Razer Kishi V2")
        self.put("strings/0x409/serialnumber", "reattach-proof")
        (self.root / "configs/c.1").mkdir()
        function = self.root / "functions/hid.usb0"
        function.mkdir()
        self.put("functions/hid.usb0/protocol", "0")
        self.put("functions/hid.usb0/subclass", "0")
        self.put("functions/hid.usb0/report_length", "8")
        # HID Game Pad: 12 buttons, padding, signed X/Y/Rx/Ry, Z/Rz triggers.
        descriptor = bytes.fromhex(
            "05 01 09 05 a1 01 05 09 19 01 29 0c 15 00 25 01 "
            "75 01 95 0c 81 02 75 04 95 01 81 03 "
            "05 01 09 30 09 31 09 33 09 34 15 81 25 7f "
            "75 08 95 04 81 02 09 32 09 35 15 00 26 ff 00 95 02 81 02 c0"
        )
        (function / "report_desc").write_bytes(descriptor)
        (self.root / "configs/c.1/hid.usb0").symlink_to(function)
        self.udc = next(Path("/sys/class/udc").iterdir()).name
        self.fd = None
        self.buttons = 0

    def put(self, path, value):
        (self.root / path).write_text(value + "\n")

    async def connect(self):
        self.put("UDC", self.udc)
        for _ in range(100):
            # Match the real USB identity, not the product name or event order.
            for event in Path("/sys/class/input").glob("event*"):
                ids = event / "device/id"
                if (ids / "vendor").read_text().strip() == "1532" and (ids / "product").read_text().strip() == "0712":
                    self.device_path = "/dev/input/" + event.name
                    self.fd = os.open("/dev/hidg0", os.O_WRONLY | os.O_NONBLOCK)
                    return self
            await asyncio.sleep(0.05)
        raise AssertionError("USB gadget did not enumerate an evdev gamepad")

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None
        self.put("UDC", "")

    def write(self, event_type, code, value):
        assert (event_type, code) == (adoption.e.EV_KEY, adoption.e.BTN_SOUTH)
        self.buttons = value

    def syn(self):
        os.write(self.fd, struct.pack("<HbbbbBB", self.buttons, 0, 0, 0, 0, 0, 0))


def journal():
    return subprocess.check_output([
        "journalctl", "-u", "adoption-profile", "--no-pager", "-o", "cat",
    ], text=True).splitlines()


def service_identity():
    return subprocess.check_output([
        "systemctl", "show", "inputplumber", "adoption-profile",
        "-p", "MainPID", "-p", "NRestarts", "-p", "InvocationID",
    ], text=True)


async def captured_button(pad, release=True):
    # An unchanged profile property alone is not a liveness assertion. Drive
    # the real evdev source -> InputPlumber target -> inputd CaptureRuntime and
    # read the native producer frames it would send to korrid.
    # Reconnect starts disarmed if capture opens on a held key. Establish a
    # neutral baseline before proving a fresh press, just like a real user.
    pad.write(adoption.e.EV_KEY, adoption.e.BTN_SOUTH, 0)
    pad.syn()
    await asyncio.sleep(0.5)
    transitions = [(1, 0x1000)] + ([(0, 0)] if release else [])
    for value, buttons in transitions:
        before = len(journal())
        pad.write(adoption.e.EV_KEY, adoption.e.BTN_SOUTH, value)
        pad.syn()
        for _ in range(100):
            for line in journal()[before:]:
                if not line.startswith("{"):
                    continue
                frame = json.loads(line)
                if frame.get("operation") == "physicalState" and frame["state"]["buttons"] == buttons:
                    break
            else:
                await asyncio.sleep(0.05)
                continue
            break
        else:
            raise AssertionError(("inputd did not capture button", buttons, journal()))


async def main():
    bus = await adoption.MessageBus(bus_type=adoption.BusType.SYSTEM).connect()
    initial_services = service_identity()
    gadget = UsbGamepad()
    pad = None
    path = None
    targets = None
    try:
        for cycle in range(10):
            pad = await gadget.connect()
            node = pad.device_path
            state = await adoption.wait_for(bus, lambda state: node in adoption.sources(state)
                                           and all(c["ProfilePath"] == adoption.PROFILE
                                                   for c in state.values() if c["SourceDevicePaths"]))
            if path is None:
                path = next(path for path, c in state.items() if node in c["SourceDevicePaths"])
                targets = state[path]["TargetDevices"]
            assert state[path]["SourceDevicePaths"] == [node], state
            assert state[path]["TargetDevices"] == targets, state
            # Wait for the unprivileged inputd capture to open the target.
            await asyncio.sleep(0.5)
            await captured_button(pad)
            if cycle == 0:
                # Force the fd-lifetime seam: a removed/replayed devnode is not
                # itself an evdev disconnect. The old provider must release its
                # EVIOGRAB before it can adopt this same kernel device again.
                info = os.stat(node)
                os.unlink(node)
                await adoption.wait_for(bus, lambda state: not adoption.sources(state))
                os.mknod(node, stat.S_IFCHR | 0o600, info.st_rdev)
                state = await adoption.wait_for(bus, lambda state: node in adoption.sources(state))
                await captured_button(pad)
                # A userspace mknod is not owned by devtmpfs. Remove it before
                # destroying uinput, or it would mask the next kernel CREATE.
                os.unlink(node)
                await adoption.wait_for(bus, lambda state: not adoption.sources(state))
            if cycle > 0:
                # Unplug while pressed. A persistent target must not carry a
                # stale held key into the replacement source's neutral baseline.
                await captured_button(pad, release=False)
            pad.close()
            pad = None
            await adoption.wait_for(bus, lambda state: not adoption.sources(state))
            if cycle == 0:
                # Force a genuine profile mismatch while the composite is
                # empty. On the next physical add inputd must apply its own
                # immutable profile, not merely trust the surviving targets.
                await adoption.call(bus, path, adoption.COMPOSITE, "LoadProfilePath", "s",
                                    [str(Path(adoption.PROFILE).with_name("default.yaml"))])
            # The Kishi briefly enumerated then vanished before its final add.
            bounce = await gadget.connect()
            await asyncio.sleep(0.01)
            bounce.close()
            await asyncio.sleep(0.1)
            print(f"PASS cycle={cycle}: physical remove/bounce; persistent targets and inputd capture", flush=True)
        assert service_identity() == initial_services
        assert any("Applied" in line for line in journal()), journal()
    finally:
        if pad is not None:
            pad.close()
        bus.disconnect()


asyncio.run(main())
