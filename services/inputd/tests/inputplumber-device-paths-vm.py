#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p 'python3.withPackages (ps: [ ps.dbus-next ])'
"""Kernel-owned xb360 nodes and installed Korri authority, in one named VM only."""

import asyncio
import fcntl
import json
import os
from pathlib import Path
import pwd
import re
import stat
import struct
import subprocess
import sys
import xml.etree.ElementTree as ET

NAME = "korri-inputplumber-device-paths"
# Check before connecting to any bus or opening any device. The VM installs both
# this marker and its test-only SMBIOS product name; no workstation fallback.
if (
    Path("/etc/hostname").read_text().strip() != NAME
    or not Path(f"/etc/{NAME}-vm").is_file()
    or Path(f"/etc/{NAME}-vm").read_text() != "isolated-kernel-proof-only\n"
    or Path("/sys/class/dmi/id/product_name").read_text().strip() != NAME
):
    raise SystemExit("refusing InputPlumber proof outside its dedicated NixOS VM")

from dbus_next import BusType, Message, MessageType, Variant
from dbus_next.aio import MessageBus

SERVICE = "org.shadowblip.InputPlumber"
PREFIX = "/org/shadowblip/InputPlumber"
MANAGER = PREFIX + "/Manager"
MANAGER_IFACE = "org.shadowblip.InputManager"
TARGET = "org.shadowblip.Input.Target"
PROPERTIES = "org.freedesktop.DBus.Properties"
ACCESS_DENIED = "org.freedesktop.DBus.Error.AccessDenied"
KEYS = {0x130, 0x131, 0x133, 0x134, 0x136, 0x137, 0x13A, 0x13B,
        0x13C, 0x13D, 0x13E, 0x2C0, 0x2C1, 0x2C2, 0x2C3}
AXES = {0, 1, 2, 3, 4, 5, 16, 17}


async def call(bus, path, interface, member, signature="", body=None,
               destination=SERVICE, denied=None):
    reply = await asyncio.wait_for(bus.call(Message(
        destination=destination, path=path, interface=interface, member=member,
        signature=signature, body=body or [],
    )), timeout=15)
    if denied is not None:
        assert reply.message_type == MessageType.ERROR, (member, "unexpected grant")
        assert reply.error_name == denied, (member, reply.error_name, reply.body)
    else:
        assert reply.message_type == MessageType.METHOD_RETURN, (
            member, reply.error_name, reply.body
        )
    return reply


async def paths(bus, target):
    reply = await call(bus, target, PROPERTIES, "Get", "ss", [TARGET, "DevicePaths"])
    value = reply.body[0]
    assert value.signature == "as", value
    nodes = value.value
    assert nodes and len(nodes) == len(set(nodes)), nodes
    assert all(re.fullmatch(r"/dev/input/event[0-9]+", node) for node in nodes), nodes
    return nodes


def validate_bulk_interfaces(path, interfaces):
    # ObjectManager must remain useful, but must not serialize this protected
    # property through its headerless getters (including broadcast signals).
    for interface, properties in interfaces.items():
        assert "DevicePaths" not in properties, (path, interface, "DevicePaths leaked")
    if TARGET in interfaces:
        properties = interfaces[TARGET]
        assert properties["Name"].signature == "s" and properties["Name"].value
        assert properties["DeviceType"].signature == "s"
        assert properties["DeviceType"].value == "xb360", (path, properties)


def ioctl_bytes(fd, number, length):
    data = bytearray(length)
    # Linux _IOR('E', number, length), used only in the Linux test guest.
    fcntl.ioctl(fd, (2 << 30) | (length << 16) | (ord("E") << 8) | number, data)
    return data


def bits(data):
    return {bit for bit in range(len(data) * 8) if data[bit // 8] & (1 << (bit % 8))}


def validate_node(node):
    before = os.lstat(node)
    assert stat.S_ISCHR(before.st_mode), node
    fd = os.open(node, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    try:
        opened = os.fstat(fd)
        assert (before.st_ino, before.st_rdev) == (opened.st_ino, opened.st_rdev)
        sysfs = (Path("/sys/class/input") / Path(node).name).resolve(strict=True)
        assert str(sysfs).startswith("/sys/devices/virtual/input/"), sysfs
        assert (sysfs / "dev").read_text().strip() == (
            f"{os.major(opened.st_rdev)}:{os.minor(opened.st_rdev)}"
        )
        device = sysfs / "device"
        assert (device / "name").read_text().strip() == "Microsoft X-Box 360 pad"
        for field in ("phys", "uniq"):
            assert (device / field).read_text().strip() == "", field
        assert struct.unpack("HHHH", ioctl_bytes(fd, 0x02, 8)) == (3, 0x045E, 0x028E, 1)
        assert ioctl_bytes(fd, 0x06, 128).split(b"\0", 1)[0] == b"Microsoft X-Box 360 pad"
        assert bits(ioctl_bytes(fd, 0x21, 96)) == KEYS
        assert bits(ioctl_bytes(fd, 0x23, 8)) == AXES
        assert 0x15 in bits(ioctl_bytes(fd, 0x20, 8)), "missing EV_FF"
        return {"node": node, "sysfs": str(sysfs), "keys": len(KEYS), "axes": len(AXES)}
    finally:
        os.close(fd)


async def child(bus, user, targets):
    identity = pwd.getpwnam(user)
    assert os.getuid() == os.geteuid() == identity.pw_uid != 0
    assert os.getgid() == os.getegid() == identity.pw_gid
    assert os.getgroups() == [], "child retained supplementary authority"
    if user != "korri-inputd":
        # 'unrelated' passes the upstream bus policy, then fails the real new
        # polkit action. The ordinary action user is blocked at the bus itself.
        denied = ACCESS_DENIED if user == "korri" else "org.freedesktop.DBus.Error.Failed"
        reply = await call(bus, targets[0], PROPERTIES, "Get", "ss",
                           [TARGET, "DevicePaths"], denied=denied)
        if user == "unrelated":
            assert reply.body == [f"Not authorized for {TARGET}.DevicePaths"], reply.body
            # This user has ordinary ObjectManager access. Require a successful
            # reply and inspect its contents; AccessDenied is not a substitute.
            snapshot = await call(bus, PREFIX, "org.freedesktop.DBus.ObjectManager",
                                  "GetManagedObjects")
            assert snapshot.signature == "a{oa{sa{sv}}}"
            objects = snapshot.body[0]
            for target in targets:
                assert TARGET in objects[target], (target, "missing ordinary target metadata")
                validate_bulk_interfaces(target, objects[target])
            print(json.dumps({"user": user, "denied": "DevicePaths",
                              "bulk_targets_without_device_paths": targets}))
        else:
            print(json.dumps({"user": user, "denied": "DevicePaths"}))
        return

    results = {target: [validate_node(node) for node in await paths(bus, target)]
               for target in targets}
    for target in targets:
        await call(bus, target, PROPERTIES, "GetAll", "s", [TARGET], denied=ACCESS_DENIED)
        await call(bus, target, PROPERTIES, "Set", "ssv",
                   [TARGET, "DevicePaths", Variant("as", [])], denied=ACCESS_DENIED)
    await call(bus, MANAGER, MANAGER_IFACE, "CreateTargetDevice", "s", ["xb360"],
               denied=ACCESS_DENIED)
    # The existing composite mutation grants must remain authorized. Ask the
    # real authority about this connection; do not create a fake composite.
    for member in ("Stop", "LoadProfilePath"):
        reply = await call(
            bus, "/org/freedesktop/PolicyKit1/Authority", "org.freedesktop.PolicyKit1.Authority",
            "CheckAuthorization", "(sa{sv})sa{ss}us",
            [["system-bus-name", {"name": Variant("s", bus.unique_name)}],
             f"org.shadowblip.Input.CompositeDevice.{member}", {}, 0, ""],
            destination="org.freedesktop.PolicyKit1",
        )
        assert reply.body[0][0], f"existing {member} polkit grant was lost"
    print(json.dumps(results))


def as_user(user, targets):
    identity = pwd.getpwnam(user)
    result = subprocess.run(
        ["/run/current-system/sw/bin/python3", str(Path(__file__).resolve()), user, *targets],
        user=identity.pw_uid, group=identity.pw_gid, extra_groups=[],
        env={"PATH": "/run/current-system/sw/bin"},
        text=True, capture_output=True, timeout=90,
    )
    assert result.returncode == 0, (user, result.stdout, result.stderr)
    return json.loads(result.stdout)


async def root(bus):
    assert os.getuid() == os.geteuid() == 0
    # Event-driven registration barriers: no sleeps or polling for target nodes.
    owner = asyncio.Event()
    registered = {}
    added_signals = []

    def signal(message):
        if message.message_type != MessageType.SIGNAL:
            return
        if message.interface == "org.freedesktop.DBus" and message.member == "NameOwnerChanged":
            if message.body[0] == SERVICE and message.body[2]:
                owner.set()
        if message.interface == "org.freedesktop.DBus.ObjectManager" and message.member == "InterfacesAdded":
            path, interfaces = message.body
            # Check outside the callback: dbus-next may log and swallow handler
            # exceptions, which would otherwise turn a leak into a passing test.
            added_signals.append((path, interfaces))
            for interface in interfaces:
                registered.setdefault((path, interface), asyncio.Event()).set()

    bus.add_message_handler(signal)
    for rule in (
        f"type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{SERVICE}'",
        f"type='signal',sender='{SERVICE}',interface='org.freedesktop.DBus.ObjectManager',member='InterfacesAdded'",
    ):
        await call(bus, "/org/freedesktop/DBus", "org.freedesktop.DBus", "AddMatch", "s", [rule],
                   destination="org.freedesktop.DBus")
    present = await call(bus, "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner",
                         "s", [SERVICE], destination="org.freedesktop.DBus")
    if not present.body[0]:
        await asyncio.wait_for(owner.wait(), timeout=30)
    snapshot = await call(bus, PREFIX, "org.freedesktop.DBus.ObjectManager", "GetManagedObjects")
    for path, interfaces in snapshot.body[0].items():
        for interface in interfaces:
            registered.setdefault((path, interface), asyncio.Event()).set()
    await asyncio.wait_for(registered.setdefault((MANAGER, MANAGER_IFACE), asyncio.Event()).wait(), 30)

    xml = await call(bus, MANAGER, "org.freedesktop.DBus.Introspectable", "Introspect")
    method = ET.fromstring(xml.body[0]).find(
        f"./interface[@name='{MANAGER_IFACE}']/method[@name='CreateTargetDevice']"
    )
    assert method is not None
    assert [(arg.get("direction", "in"), arg.get("type")) for arg in method.findall("arg")] == [
        ("in", "s"), ("out", "s")
    ], "upstream CreateTargetDevice signature changed"

    pid = subprocess.check_output(
        ["systemctl", "show", "inputplumber.service", "--property=MainPID", "--value"], text=True
    ).strip()
    assert b"INSECURE_DISABLE_POLKIT=" not in Path(f"/proc/{pid}/environ").read_bytes()
    targets = []
    for _ in range(2):
        reply = await call(bus, MANAGER, MANAGER_IFACE, "CreateTargetDevice", "s", ["xb360"])
        assert reply.signature == "s"
        target = reply.body[0]
        assert target.startswith(PREFIX + "/devices/target/"), target
        targets.append(target)
        await asyncio.wait_for(registered.setdefault((target, TARGET), asyncio.Event()).wait(), 30)
        assert any(path == target and TARGET in interfaces for path, interfaces in added_signals), (
            target, "missing InterfacesAdded target payload"
        )
    assert targets[0] != targets[1]
    subprocess.run(["udevadm", "settle", "--timeout=30"], check=True, timeout=35)
    root_paths = [set(await paths(bus, target)) for target in targets]
    assert root_paths[0].isdisjoint(root_paths[1]), root_paths
    proof = as_user("korri-inputd", targets)
    for target, expected in zip(targets, root_paths):
        assert {record["node"] for record in proof[target]} == expected
    print(json.dumps({"inputd_kernel_reads": proof}, sort_keys=True))
    for user in ("unrelated", "korri"):
        print(json.dumps(as_user(user, targets)))
    for path, interfaces in added_signals:
        validate_bulk_interfaces(path, interfaces)
    print(json.dumps({"interfaces_added_without_device_paths": targets}))
    # No event injection, permission overrides, or controller identity changes.
    print("PASS: two kernel targets; inputd Get/read granted; unrelated Get and inputd GetAll/Set/Create denied; ObjectManager replies/signals omit DevicePaths")


async def main():
    bus = await MessageBus(bus_type=BusType.SYSTEM).connect()
    try:
        if len(sys.argv) == 1:
            await root(bus)
        else:
            await child(bus, sys.argv[1], sys.argv[2:])
    finally:
        bus.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
