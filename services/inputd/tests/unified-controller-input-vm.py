#!/usr/bin/env python3
"""Actual receiver + kernel seats, not a capture/korrid/browser E2E model.

The test supplies canonical coordinator/remote wire packets at their production
socket boundaries. All device operations are confined to this named NixOS VM.
"""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import stat
import struct
import subprocess
import sys
import time

NAME = "korri-unified-controller-input"
if (
    Path("/etc/hostname").read_text().strip() != NAME
    or Path(f"/etc/{NAME}-vm").read_text() != "isolated-kernel-proof-only\n"
    or Path("/sys/class/dmi/id/product_name").read_text().strip() != NAME
):
    raise SystemExit("refusing controller proof outside its dedicated NixOS VM")

ROOT = Path("/run/korri-input-seat")
CONTROL = "control.sock"
MIRROR = "sunshine-input-seat.sock"
CLAIM = ROOT / "sunshine-active-launch.json"
LAUNCH = "0123456789abcdef0123456789abcdef"
NEXT = "fedcba9876543210fedcba9876543210"
NEUTRAL = dict(buttons=0, left_trigger=0, right_trigger=0, left_stick_x=0,
               left_stick_y=0, right_stick_x=0, right_stick_y=0)
BUTTONS = {0x10: 0x13B, 0x20: 0x13A, 0x40: 0x13D, 0x80: 0x13E,
           0x100: 0x136, 0x200: 0x137, 0x400: 0x13C, 0x1000: 0x130,
           0x2000: 0x131, 0x4000: 0x133, 0x8000: 0x134}
AXES = {0, 1, 2, 3, 4, 5, 16, 17}


def agent_main():
    """A real unprivileged process: no inherited sockets or extra groups."""
    uid, gid = map(int, sys.argv[2:4])
    groups = list(map(int, sys.argv[4:]))
    assert os.getuid() == os.geteuid() == uid != 0
    assert os.getgid() == os.getegid() == gid
    assert sorted(os.getgroups()) == sorted(groups)
    sockets = {}
    for line in sys.stdin:
        command = json.loads(line)
        try:
            operation = command["op"]
            result = None
            if operation == "connect":
                assert command["path"] in (CONTROL, MIRROR)
                connection = socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET)
                connection.settimeout(3)
                try:
                    connection.connect(str(ROOT / command["path"]))
                    _, peer_uid, peer_gid = struct.unpack(
                        "3i", connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
                    assert (peer_uid, peer_gid) == (0, 0), "receiver is not root:root"
                except BaseException:
                    connection.close()
                    raise
                sockets[command["id"]] = connection
            elif operation == "exchange":
                connection = sockets[command["id"]]
                connection.sendall(bytes.fromhex(command["packet"]))
                result = connection.recv(65537).hex()
            elif operation == "close":
                sockets.pop(command["id"]).close()
            elif operation == "claim":
                metadata = CLAIM.stat()
                assert (metadata.st_uid, metadata.st_gid, stat.S_IMODE(metadata.st_mode)) == (0, 980, 0o640)
                result = json.loads(CLAIM.read_text())
            elif operation == "read-node":
                fd = os.open(command["path"], os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
                os.close(fd)
            else:
                raise AssertionError("unknown helper operation")
            print(json.dumps({"ok": result}), flush=True)
        except OSError as error:
            # Never echo a command/packet/claim: it may contain a mirror token.
            print(json.dumps({"errno": error.errno}), flush=True)
    for connection in sockets.values():
        connection.close()


class Agent:
    def __init__(self, uid, gid, groups=()):
        self.process = subprocess.Popen(
            [sys.executable, __file__, "--agent", str(uid), str(gid), *map(str, groups)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
            user=uid, group=gid, extra_groups=list(groups),
        )

    def call(self, op, **kwargs):
        self.process.stdin.write(json.dumps(dict(op=op, **kwargs)) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        assert line, f"credential helper exited during {op}"
        return json.loads(line)

    def ok(self, op, **kwargs):
        result = self.call(op, **kwargs)
        assert "ok" in result, (op, result)
        return result["ok"]

    def connect(self, handle, path=CONTROL):
        self.ok("connect", id=handle, path=path)

    def exchange(self, handle, packet):
        return bytes.fromhex(self.ok("exchange", id=handle, packet=packet.hex()))

    def close(self, handle):
        self.ok("close", id=handle)

    def stop(self):
        self.process.stdin.close()
        try:
            assert self.process.wait(timeout=5) == 0
        finally:
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait()


def coordination(operation, **fields):
    # contracts/input/src/lib.rs: version byte + serde-tagged JSON, no newline.
    return b"\x02" + json.dumps(dict(operation=operation, **fields)).encode()


def binary(operation, launch=LAUNCH):
    return bytes([1, operation]) + launch.encode()


def ioctl_bytes(fd, number, length):
    data = bytearray(length)
    fcntl.ioctl(fd, (2 << 30) | (length << 16) | (ord("E") << 8) | number, data)
    return data


def bits(data):
    return {bit for bit in range(len(data) * 8) if data[bit // 8] & (1 << (bit % 8))}


def discover():
    found = {}
    for event in Path("/sys/class/input").glob("event*"):
        name = (event / "device/name").read_text().strip()
        match = re.fullmatch(r"Korri Seat P([1-9][0-9]*)", name)
        if match:
            slot = int(match[1])
            assert slot not in found, "duplicate seat identity"
            found[slot] = Path("/dev/input") / event.name
    return found


class KernelSeats:
    def __init__(self, count):
        nodes = discover()
        assert set(nodes) == set(range(1, count + 1)), nodes
        self.fds = {}
        self.fingerprints = {}
        for slot, node in nodes.items():
            before = node.lstat()
            assert stat.S_ISCHR(before.st_mode)
            assert (before.st_uid, before.st_gid, stat.S_IMODE(before.st_mode)) == (0, 1000, 0o660)
            fd = os.open(node, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
            self.fds[slot] = fd
            after = os.fstat(fd)
            assert (before.st_ino, before.st_rdev) == (after.st_ino, after.st_rdev)
            sysfs = (Path("/sys/class/input") / node.name).resolve(strict=True)
            assert str(sysfs).startswith("/sys/devices/virtual/input/")
            assert (sysfs / "dev").read_text().strip() == f"{os.major(after.st_rdev)}:{os.minor(after.st_rdev)}"
            assert (sysfs / "device/phys").read_text().strip() == f"korri/input-seat/p{slot}"
            assert (sysfs / "device/uniq").read_text().strip() == ""
            assert struct.unpack("HHHH", ioctl_bytes(fd, 0x02, 8)) == (3, 0x045E, 0x028E, 1)
            assert ioctl_bytes(fd, 0x06, 128).split(b"\0", 1)[0] == f"Korri Seat P{slot}".encode()
            assert ioctl_bytes(fd, 0x07, 128).split(b"\0", 1)[0] == f"korri/input-seat/p{slot}".encode()
            assert bits(ioctl_bytes(fd, 0x21, 96)) == set(BUTTONS.values())
            assert bits(ioctl_bytes(fd, 0x23, 8)) == AXES
            assert bits(ioctl_bytes(fd, 0x20, 8)) == {0, 1, 3}
            for axis in AXES:
                _, minimum, maximum, fuzz, flat, resolution = struct.unpack("6i", ioctl_bytes(fd, 0x40 + axis, 24))
                expected = (-32768, 32767, 4096) if axis in (0, 1, 3, 4) else ((0, 255, 0) if axis in (2, 5) else (-1, 1, 0))
                assert (minimum, maximum, flat) == expected, (slot, axis)
                assert (fuzz, resolution) == (0, 0)
            self.fingerprints[slot] = (str(node), str(sysfs), after.st_ino, after.st_rdev)
        assert len({value[3] for value in self.fingerprints.values()}) == count

    def stable(self):
        assert set(discover()) == set(self.fds)
        for slot, (node, sysfs, inode, rdev) in self.fingerprints.items():
            metadata = os.stat(node)
            assert (metadata.st_ino, metadata.st_rdev) == (inode, rdev), slot
            assert str((Path("/sys/class/input") / Path(node).name).resolve(strict=True)) == sysfs

    def state(self, slot):
        fd = self.fds[slot]
        keys = bits(ioctl_bytes(fd, 0x18, 96))
        axes = {axis: struct.unpack("6i", ioctl_bytes(fd, 0x40 + axis, 24))[0] for axis in AXES}
        return keys, axes

    def expect(self, slot, state=NEUTRAL):
        buttons = state["buttons"]
        keys = {key for mask, key in BUTTONS.items() if buttons & mask}
        axes = {0: state["left_stick_x"], 1: state["left_stick_y"],
                2: state["left_trigger"], 3: state["right_stick_x"],
                4: state["right_stick_y"], 5: state["right_trigger"],
                16: int(bool(buttons & 8)) - int(bool(buttons & 4)),
                17: int(bool(buttons & 2)) - int(bool(buttons & 1))}
        assert self.state(slot) == (keys, axes), (slot, self.state(slot), (keys, axes))

    def neutral(self):
        for slot in self.fds:
            self.expect(slot)

    def close(self):
        for fd in self.fds.values():
            os.close(fd)
        self.fds.clear()


def held(index):
    return dict(buttons=[0x1001, 0x2002, 0x4004, 0x8008, 0x110, 0x220][index - 1],
                left_trigger=20 + index, right_trigger=100 + index,
                left_stick_x=10000 + index, left_stick_y=-11000 - index,
                right_stick_x=-12000 - index, right_stick_y=13000 + index)


class Driver:
    def __init__(self, control, sunshine):
        self.control = control
        self.sunshine = sunshine
        self.physical = {}
        self.remote = {}
        self.events = []
        self.claim = None
        self.last_refresh = time.monotonic()

    def request(self, operation, failure=None, **fields):
        raw = self.control.exchange("coordinator", coordination(operation, **fields))
        assert raw[:1] == b"\x02" and len(raw) <= 65536
        reply = json.loads(raw[1:])
        assert reply["failure"] == failure, (operation, reply)
        self.events.extend(reply["remoteEvents"])
        return reply

    def hello(self):
        self.control.connect("coordinator")
        return self.request("hello")

    def physical_connect(self, identity, state=NEUTRAL):
        reply = self.request("physicalConnected", deviceId=identity,
                             name="VM boundary fixture", state=state)
        self.physical[identity] = state.copy()
        return reply["slot"]

    def physical_state(self, identity, state):
        reply = self.request("physicalState", deviceId=identity, state=state)
        self.physical[identity] = state.copy()
        return reply["slot"]

    def physical_disconnect(self, identity):
        self.request("physicalDisconnected", deviceId=identity)
        self.physical.pop(identity)

    def start(self, launch=LAUNCH):
        self.control.connect("lease")
        assert self.control.exchange("lease", binary(1, launch)) == b"\x01\x00\x00"
        self.claim = self.sunshine.ok("claim")
        assert set(self.claim) == {"launchId", "generation", "mirrorToken"}
        assert self.claim["launchId"] == launch
        assert re.fullmatch("[0-9a-f]{64}", self.claim["mirrorToken"])
        assert_permissions()

    def frame(self, controller, kind="source-state", state=NEUTRAL,
              agent=None, token=None, launch=None):
        frame = dict(kind=kind, launchId=launch or self.claim["launchId"], controllerNumber=controller)
        if kind == "source-state":
            # Existing Sunshine mirror uses camelCase and positive Y up.
            frame.update(buttons=state["buttons"], leftTrigger=state["left_trigger"],
                         rightTrigger=state["right_trigger"], leftStickX=state["left_stick_x"],
                         leftStickY=-state["left_stick_y"], rightStickX=state["right_stick_x"],
                         rightStickY=-state["right_stick_y"])
        packet = json.dumps(dict(mirrorToken=token or self.claim["mirrorToken"], frame=frame)).encode() + b"\n"
        sender = agent or self.sunshine
        sender.connect("frame", MIRROR)
        # EOF is the receiver's processing barrier (there is no mirror reply).
        outcome = sender.call("exchange", id="frame", packet=packet.hex())
        # A rejected peer can close before send or while recv is pending.
        # State assertions below prove that neither denial path applied input.
        assert outcome in ({"ok": ""}, {"errno": 104}, {"errno": 32}), ("unexpected mirror response", outcome)
        sender.close("frame")

    def remote_connect(self, controller):
        self.frame(controller, "source-connected")
        # A connect notification alone has no measured baseline. Production
        # publishes the source only after its first full state packet.
        self.remote_state(controller, NEUTRAL)
        reply = self.request("poll")
        sources = [source for source in reply["remoteSources"] if source["controllerNumber"] == controller]
        assert len(sources) == 1
        return sources[0]["slot"]

    def remote_state(self, controller, state):
        self.frame(controller, state=state)
        self.remote[controller] = state.copy()

    def remote_disconnect(self, controller):
        self.frame(controller, "source-disconnected")
        self.remote.pop(controller)
        self.request("poll")

    def heartbeat(self):
        if time.monotonic() - self.last_refresh >= 0.2:
            for identity, state in list(self.physical.items()):
                self.physical_state(identity, state)
            for controller, state in list(self.remote.items()):
                self.remote_state(controller, state)
            self.last_refresh = time.monotonic()
        return self.request("poll")

    def wait(self, predicate, timeout=3, heartbeat=True):
        deadline = time.monotonic() + timeout
        while True:
            if heartbeat:
                self.heartbeat()
            if predicate():
                return
            assert time.monotonic() < deadline, "receiver condition timed out"
            time.sleep(0.02)

    def maintain(self, seconds):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            self.heartbeat()
            time.sleep(0.02)


def assert_permissions():
    directory = ROOT.stat()
    assert (directory.st_uid, stat.S_IMODE(directory.st_mode)) == (0, 0o711)
    for filename, gid in ((CONTROL, 976), (MIRROR, 980)):
        metadata = (ROOT / filename).lstat()
        assert stat.S_ISSOCK(metadata.st_mode)
        assert (metadata.st_uid, metadata.st_gid, stat.S_IMODE(metadata.st_mode)) == (0, gid, 0o660)


def proof():
    assert os.getuid() == os.geteuid() == 0
    agents = []
    kernels = []

    def agent(uid, gid, groups=()):
        value = Agent(uid, gid, groups)
        agents.append(value)
        return value

    def seats(count):
        value = KernelSeats(count)
        kernels.append(value)
        return value

    try:
        manifest = json.loads(Path(f"/etc/{NAME}-sources.json").read_text())
        binary_path = Path(manifest["inputdPackage"]) / "bin/korri-input-seat-receiver"
        with binary_path.open("rb") as executable:
            binary_hash = hashlib.file_digest(executable, "sha256").hexdigest()
        print(json.dumps({"evaluated_source_hashes": manifest, "receiver_binary_sha256": binary_hash}), flush=True)
        kernel = seats(4)
        kernel.neutral()
        kernel.close()
        control = agent(976, 976)
        sunshine = agent(1000, 980)
        unrelated = agent(1001, 1001)
        wrong_uid = agent(1001, 976)
        wrong_gid = agent(976, 1001, (976,))
        wrong_mirror_uid = agent(1001, 980)
        wrong_mirror_gid = agent(1000, 1001, (980,))
        reader = agent(1000, 1000)
        for intruder in (wrong_uid, wrong_gid):
            intruder.connect("denied")
            assert intruder.exchange("denied", coordination("hello")) == b"\x01\x01\x02"
            intruder.close("denied")
        assert unrelated.call("connect", id="denied", path=CONTROL) == {"errno": 13}
        control.connect("unready")
        assert control.exchange("unready", binary(1)) == b"\x01\x01\x04"
        control.close("unready")
        driver = Driver(control, sunshine)
        first = driver.hello()
        assert first["count"] == 4 and first["session"] is None and first["recoveryRequired"]
        driver.request("beginSession", failure="notReady", launchId=LAUNCH)
        assert driver.request("applyCount", count=6)["count"] == 6
        kernel = seats(6)
        kernel.neutral()
        node = kernel.fingerprints[6][0]
        reader.ok("read-node", path=node)
        assert unrelated.call("read-node", path=node) == {"errno": 13}
        driver.request("poll")
        control.connect("duplicate")
        denied = control.exchange("duplicate", coordination("hello"))
        assert json.loads(denied[1:])["failure"] == "active"
        control.close("duplicate")
        driver.request("beginSession", launchId=LAUNCH)
        driver.request("applyCount", failure="active", count=5)
        driver.request("route", failure="stale", launchId=NEXT)
        driver.start()
        assert unrelated.call("claim") == {"errno": 13}
        # These are explicitly test producer identities, not invented InputPlumber
        # provenance. Capture/uniq/phys validation belongs to the separate proof.
        p1, p2, p3, overflow = (f"unified-controller-fixture-{i}" for i in range(4))
        assert driver.physical_connect(p1, held(1)) == 1
        assert driver.remote_connect(0) == 2
        assert driver.physical_connect(p2) == 3
        assert driver.remote_connect(1) == 4
        assert driver.physical_connect(p3) == 5
        assert driver.remote_connect(2) == 6
        assignments = {p1: 1, p2: 3, p3: 5}
        remote_slots = {0: 2, 1: 4, 2: 6}
        kernel.neutral()  # begin + START does not imply gameplay routing
        driver.request("route", launchId=LAUNCH)

        def all_states(neutral=False):
            for identity, slot in assignments.items():
                driver.physical_state(identity, NEUTRAL if neutral else held(slot))
            for controller, slot in remote_slots.items():
                driver.remote_state(controller, NEUTRAL if neutral else held(slot))
            driver.request("poll")

        all_states()
        kernel.neutral()  # held baselines must not arm gameplay
        all_states(True)
        all_states()
        for slot in range(1, 7):
            kernel.expect(slot, held(slot))
        driver.maintain(1.5)  # quiet held sources refreshed, not stale after 1250ms
        for slot in range(1, 7):
            kernel.expect(slot, held(slot))
        # Both exact credentials and mirror token/launch authority are required.
        for intruder in (wrong_mirror_uid, wrong_mirror_gid):
            driver.frame(0, state=NEUTRAL, agent=intruder)
        driver.frame(0, state=NEUTRAL, token="0" * 64)
        driver.frame(0, state=NEUTRAL, launch=NEXT)
        driver.request("poll")
        kernel.expect(2, held(2))
        assert control.exchange("lease", binary(3, NEXT)) == b"\x01\x01\x05"
        assert control.exchange("lease", binary(3)) == b"\x01\x00\x00"
        for slot in (2, 4, 6):
            kernel.expect(slot)
        for slot in (1, 3, 5):
            kernel.expect(slot, held(slot))
        for controller, slot in remote_slots.items():
            driver.remote_state(controller, held(slot))
            kernel.expect(slot)
            driver.remote_state(controller, NEUTRAL)
            driver.remote_state(controller, held(slot))
            kernel.expect(slot, held(slot))
        driver.request("route", launchId=None)
        kernel.neutral()
        driver.request("applyCount", failure="active", count=5)  # paused is active
        driver.request("route", launchId=LAUNCH)
        all_states()
        kernel.neutral()
        all_states(True)
        all_states()
        kernel.stable()
        driver.physical_disconnect(p1)
        kernel.expect(1)
        assert driver.physical_connect(p1, held(1)) == 1
        driver.physical_state(p1, held(1))
        kernel.expect(1)
        driver.physical_state(p1, NEUTRAL)
        driver.physical_state(p1, held(1))
        kernel.expect(1, held(1))
        driver.physical_disconnect(p1)
        driver.remote_disconnect(1)
        kernel.expect(1)
        kernel.expect(4)
        assert driver.physical_connect(overflow) is None
        driver.physical_state(overflow, NEUTRAL)
        driver.physical_state(overflow, held(1))
        kernel.expect(1)
        kernel.expect(4)
        # Lease EOF must neutralize remote seats WITHOUT ending reservations.
        old_token = driver.claim["mirrorToken"]
        control.close("lease")
        driver.remote.clear()
        driver.wait(lambda: not CLAIM.exists())
        assert driver.request("poll")["session"] == LAUNCH
        for slot in (2, 4, 6):
            kernel.expect(slot)
        kernel.expect(3, held(3))
        kernel.expect(5, held(5))
        assert driver.physical_state(overflow, NEUTRAL) is None
        driver.request("applyCount", failure="active", count=5)
        driver.start()
        assert driver.claim["mirrorToken"] != old_token
        driver.frame(0, token=old_token)
        assert driver.request("poll")["remoteSources"] == []
        assert driver.remote_connect(0) == 2  # same source reclaims reserved slot
        driver.remote_state(0, NEUTRAL)
        driver.remote_state(0, held(2))
        kernel.expect(2, held(2))
        assert control.exchange("lease", binary(2, NEXT)) == b"\x01\x01\x05"
        driver.request("endSession", failure="stale", launchId=NEXT)
        driver.request("endSession", launchId=LAUNCH)
        driver.remote.clear()
        control.close("lease")
        kernel.neutral()
        kernel.stable()
        assert not CLAIM.exists() and not (ROOT / MIRROR).exists()
        # Connected sources retain slots; disconnected reservations free only at
        # the session boundary. The connected overflow source can now take P1.
        assert driver.physical_state(p2, NEUTRAL) == 3
        assert driver.physical_state(p3, NEUTRAL) == 5
        assert driver.physical_state(overflow, NEUTRAL) == 1
        driver.request("beginSession", launchId=NEXT)
        driver.request("route", launchId=NEXT)
        driver.start(NEXT)
        assert driver.remote_connect(0) == 2
        driver.physical_state(p2, NEUTRAL)
        driver.physical_state(p2, held(3))
        driver.remote_state(0, NEUTRAL)
        driver.remote_state(0, held(2))
        kernel.expect(3, held(3))
        kernel.expect(2, held(2))
        # Stop refreshing one physical and one remote source, while coordinator
        # and the other physical sources remain healthy.
        driver.physical.pop(p2)
        driver.remote.pop(0)
        driver.maintain(1.5)
        kernel.expect(3)
        kernel.expect(2)
        assert driver.request("poll")["remoteSources"] == []
        assert driver.physical_connect(p2) == 3
        driver.physical_state(p2, NEUTRAL)
        driver.physical_state(p2, held(3))
        # Coordinator heartbeat loss (socket stays open) must fail closed.
        time.sleep(1.5)
        kernel.neutral()
        driver.wait(lambda: not CLAIM.exists(), heartbeat=False)
        control.close("coordinator")
        control.close("lease")
        driver.physical.clear()
        driver.remote.clear()
        recovered = driver.hello()
        assert recovered["recoveryRequired"] and recovered["session"] == NEXT
        driver.request("applyCount", failure="active", count=5)
        driver.request("route", failure="notReady", launchId=NEXT)
        driver.request("beginSession", failure="notReady", launchId=LAUNCH)
        driver.request("beginSession", launchId=NEXT)
        assert not driver.request("poll")["recoveryRequired"]
        assert driver.physical_connect("unified-controller-replacement") == 4
        assert driver.physical_connect(p2, held(3)) == 3
        driver.request("route", launchId=NEXT)
        driver.physical_state(p2, held(3))
        kernel.expect(3)
        driver.physical_state(p2, NEUTRAL)
        driver.physical_state(p2, held(3))
        kernel.expect(3, held(3))
        # Malformed coordinator data is also loss, not a permissive fallback.
        assert control.exchange("coordinator", b"\x02{invalid") == b""
        control.close("coordinator")
        driver.physical.clear()
        kernel.neutral()
        recovered = driver.hello()
        assert recovered["session"] == NEXT and recovered["recoveryRequired"]
        driver.request("applyCount", failure="active", count=5)
        driver.request("endSession", launchId=NEXT)
        kernel.stable()
        kernel.close()
        reply = driver.request("applyCount", count=5)
        assert reply["count"] == 5 and not reply["recoveryRequired"]
        kernel = seats(5)
        kernel.neutral()
        driver.request("poll")
        # With no session, loss frees the slot immediately, not the device.
        assert driver.physical_connect(p1) == 1
        driver.physical_disconnect(p1)
        assert driver.physical_connect(p2) == 1
        driver.physical_disconnect(p2)
        kernel.stable()
        kernel.close()
        assert driver.request("applyCount", count=6)["count"] == 6
        kernel = seats(6)
        kernel.neutral()
        # Check actual ordered remote transitions, not just current snapshots.
        kinds = [event["kind"] for event in driver.events]
        assert "connected" in kinds and "state" in kinds and "disconnected" in kinds
        first_connected = next(i for i, event in enumerate(driver.events)
                               if event["kind"] == "connected" and event["source"]["controllerNumber"] == 0)
        first_state = next(i for i, event in enumerate(driver.events)
                           if event["kind"] == "state" and event["source"]["controllerNumber"] == 0)
        first_disconnected = next(i for i, event in enumerate(driver.events)
                                  if event["kind"] == "disconnected" and event["controllerNumber"] == 0)
        assert first_connected < first_state < first_disconnected
        control.close("coordinator")
        kernel.close()
        old_pid = subprocess.check_output(
            ["systemctl", "show", "korri-input-seat-receiver.service", "-p", "MainPID", "--value"], text=True).strip()
        # ExecStartPre repeats the VM guard before the replacement binary can
        # create devices. No guest compilation, bundle edits, or permissions edits.
        subprocess.run(["systemctl", "restart", "korri-input-seat-receiver.service"],
                       check=True, timeout=30)
        new_pid = subprocess.check_output(
            ["systemctl", "show", "korri-input-seat-receiver.service", "-p", "MainPID", "--value"], text=True).strip()
        assert old_pid != new_pid and new_pid != "0"
        kernel = seats(4)
        kernel.neutral()
        initial = driver.hello()
        assert initial["count"] == 4 and initial["session"] is None and initial["recoveryRequired"]
        driver.request("beginSession", failure="notReady", launchId=NEXT)
        kernel.close()
        assert driver.request("applyCount", count=6)["count"] == 6
        kernel = seats(6)
        kernel.neutral()
        control.close("coordinator")
        print("PASS: root receiver; exact UID/GID + token authority; real 4/6/5/6 kernel seats and capabilities; mixed allocation; held/neutral/rearm; timeout and malformed loss; lease/session reservations; connected retention; active resize/recovery denial; idle resize; receiver restart reconciliation")
        print("BOUNDARY: canonical physical packets are fixtures, not InputPlumber capture; no korrid host/native/browser path or bundle signature acceptance is claimed")
    finally:
        for kernel in kernels:
            kernel.close()
        for agent in agents:
            agent.stop()


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--agent":
        agent_main()
    elif sys.argv[1:] == ["--assert-absent"]:
        assert not discover(), "receiver shutdown left live kernel seats"
        assert not (ROOT / CONTROL).exists()
        assert not CLAIM.exists() and not (ROOT / MIRROR).exists()
        print("PASS: receiver shutdown released all kernel seats and authority artifacts")
    else:
        assert len(sys.argv) == 1
        proof()
