"""Number 5 of ADR 0062: from a key press to the input packet that leaves the viewer.

The press is `press.c`: a key through the XTEST extension of the private
pair's X server, on the viewer's own window. The packet is the first send of
an ENet command on Moonlight's keyboard channel, from the viewer to the
control port of its Sunshine, on the pair's loopback. The kernel stamps each
packet when the viewer's send puts it on loopback.

`Capture` belongs to the viewer's root agent and runs only inside the pair's
network namespace. Everything else here is pure: the controller matches
presses to packets from the evidence, so a run's saved `input.json` can be
scored again.
"""

import ctypes
import math
from os import readlink as os_readlink
import socket
import statistics
import struct
import threading
import time

# The schedule of one run's presses (press.c). The gaps are not a whole number
# of any frame period, so a press does not lock to the viewer's frame loop.
PRESSES = 20
HOLD_MS = 30
GAP_MS = 200
JITTER_MS = 100
# How long the capture stays open after the last release: the window in which
# the last press's packets count.
TAIL_SECONDS = 0.5
# An X server repeats a held key after its repeat delay (660 ms by default):
# a hold this long or longer could add keys that no press sent.
HOLD_LIMIT_MS = 500
# A wall clock that moves against the monotonic clock by more than this
# during the presses has been stepped: kernel packet times are wall-clock.
# A press is answered in about 50 us, so the limit is that small; each side
# reads the wall clock between two monotonic reads to stay inside it.
CLOCK_STEP_LIMIT_NS = 50_000
METHOD = "x11-xtest-key"

# ENet as moonlight-common-c carries it (enet/include/enet/protocol.h). Header
# fields are big-endian.
HEADER_COMPRESSED = 1 << 14
HEADER_SENT_TIME = 1 << 15
COMMAND_MASK = 0x0F
COMMAND_SIZES = (0, 8, 48, 44, 8, 4, 6, 8, 24, 8, 12, 16, 24)
SEND_RELIABLE = 6
# The send commands, and where their dataLength field lies in the command.
SEND_DATA_LENGTH = {6: 4, 7: 6, 8: 6, 9: 6, 12: 6}
# moonlight-common-c src/Limelight-internal.h: CTRL_CHANNEL_KEYBOARD. Only
# keyboard events travel on it, and the payload above ENet is encrypted.
KEYBOARD_CHANNEL = 0x02

# Linux packet sockets (linux/if_ether.h, asm-generic/socket.h).
ETH_P_IP = 0x0800
SO_ATTACH_FILTER = 26
SO_TIMESTAMPNS = 35


class PressFailed(ValueError):
    """The measure itself failed: no statement about the viewer can be made."""


def clock_offset_ns() -> int:
    """The wall clock against the monotonic clock: it changes only when the wall clock is stepped.

    The wall clock is read between two monotonic reads, five times; the
    narrowest pair wins, so a preemption between two reads does not look
    like a step.
    """
    best: tuple[int, int] | None = None
    for _ in range(5):
        before = time.clock_gettime_ns(time.CLOCK_MONOTONIC)
        wall = time.clock_gettime_ns(time.CLOCK_REALTIME)
        after = time.clock_gettime_ns(time.CLOCK_MONOTONIC)
        if best is None or after - before < best[0]:
            best = (after - before, wall - (before + after) // 2)
    assert best is not None
    return best[1]


def press_log(text: str) -> dict:
    """press.c's lines: the target window, each press and release, and whether it ended."""
    target = None
    events = []
    done = False
    for line in text.splitlines():
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "target" and len(parts) == 4:
            target = {"window": int(parts[1]), "owner_pid": int(parts[2]), "keycode": int(parts[3])}
        elif parts[0] in ("press", "release") and len(parts) == 5:
            events.append(
                {
                    "kind": parts[0],
                    "index": int(parts[1]),
                    "realtime_ns": int(parts[2]),
                    "monotonic_ns": int(parts[3]),
                    "focused": parts[4] == "1",
                }
            )
        elif parts == ["done"]:
            done = True
        else:
            raise PressFailed(f"The press program wrote a line that it never writes: {line!r}.")
    return {"target": target, "events": events, "done": done}


def enet_commands(payload: bytes) -> list[tuple[int, int, int]] | None:
    """(command, channel, reliable sequence) of each command in one ENet datagram; None when it cannot be read."""
    if len(payload) < 2:
        return None
    peer = struct.unpack_from(">H", payload)[0]
    if peer & HEADER_COMPRESSED:
        return None
    offset = 4 if peer & HEADER_SENT_TIME else 2
    commands = []
    while offset < len(payload):
        if offset + 4 > len(payload):
            return None
        kind = payload[offset] & COMMAND_MASK
        if not 0 < kind < len(COMMAND_SIZES):
            return None
        size = COMMAND_SIZES[kind]
        if offset + size > len(payload):
            return None
        if kind in SEND_DATA_LENGTH:
            size += struct.unpack_from(">H", payload, offset + SEND_DATA_LENGTH[kind])[0]
            if offset + size > len(payload):
                return None
        commands.append((kind, payload[offset + 1], struct.unpack_from(">H", payload, offset + 2)[0]))
        offset += size
    return commands


def keyboard_sends(packets: list[dict]) -> tuple[list[int], int]:
    """The times of the keyboard channel's sends, each reliable command once, and the count of unreadable datagrams.

    ENet sends a reliable command again until it is acknowledged, with the
    same sequence number. Only the first send left the viewer for the press.
    """
    seen: set[int] = set()
    times = []
    unreadable = 0
    for packet in sorted(packets, key=lambda row: row["ns"]):
        commands = enet_commands(bytes.fromhex(packet["payload"]))
        if commands is None:
            unreadable += 1
            continue
        for kind, channel, sequence in commands:
            if channel != KEYBOARD_CHANNEL or kind not in SEND_DATA_LENGTH:
                continue
            if kind == SEND_RELIABLE:
                if sequence in seen:
                    continue
                seen.add(sequence)
            times.append(packet["ns"])
    return times, unreadable


# How a packet was given to its key event (`viewer.input.attribution`).
ORDER, INTERVAL = "order", "interval"


def in_order(events: list[dict], sends: list[int]) -> list[dict] | None:
    """Each measured press with its packets, when the viewer sent one packet for each key event, in order.

    Moonlight sends one keyboard command for each key event that its window
    gets, in the order of the events: one input queue, one ENet channel. A
    packet can still wait for the next event and leave in one datagram with
    that event's packet: on aka on 2026-11-14 UTC a key-up left with the next
    key-down, 173 ms late, and a key-down left with its own key-up, 30.1 ms
    late. The order, not the time, then says which packet answers which key.

    None when the counts or the times do not fit:
    - another number of packets; two more are the warm-up's only when both
      come before the first measured press;
    - a packet before its own event;
    - a packet at or after the next event that does not share its datagram
      (the same kernel time) with the packet after it;
    - a packet at or after the event two places later.
    A lost send and an extra one give the same count; without these rules
    every packet after the loss would answer the key before its own, by one
    place for a lost send and by two for a lost press. `sends` are the
    keyboard sends from the warm-up press on, in time order.
    """
    measured = sorted((row for row in events if row["index"] >= 0), key=lambda row: row["realtime_ns"])
    if (
        len(sends) == len(measured) + 2
        and measured
        and sends[1] < measured[0]["realtime_ns"]
    ):
        sends = sends[2:]
    if len(sends) != len(measured):
        return None
    for place, (ns, row) in enumerate(zip(sends, measured)):
        if ns < row["realtime_ns"]:
            return None
        if place + 2 < len(measured) and ns >= measured[place + 2]["realtime_ns"]:
            return None
        waited = place + 1 < len(measured) and ns >= measured[place + 1]["realtime_ns"]
        if waited and (place + 1 >= len(sends) or sends[place + 1] != ns):
            return None
    if [row["kind"] for row in measured] != ["press", "release"] * (len(measured) // 2):
        return None
    return [
        {
            "index": press["index"],
            "packets": 2,
            "latency_ms": (down - press["realtime_ns"]) / 1e6,
            "release_latency_ms": (up - release["realtime_ns"]) / 1e6,
        }
        for press, release, down, up in zip(measured[::2], measured[1::2], sends[::2], sends[1::2])
    ]


def by_interval(events: list[dict], sends: list[int], end_ns: int) -> list[dict]:
    """Each measured press with the keyboard sends in its interval: from it to the next press, the last to `end_ns`.

    For a viewer that did not send one packet for each key event. A press is
    answered when its interval holds exactly two sends, the second one not
    before the release: the key down and the key up. Any other count leaves
    the press unanswered, and no packet is guessed for it.
    """
    presses = [row for row in events if row["kind"] == "press" and row["index"] >= 0]
    releases = {row["index"]: row for row in events if row["kind"] == "release" and row["index"] >= 0}
    rows = []
    for number, press in enumerate(presses):
        until = presses[number + 1]["realtime_ns"] if number + 1 < len(presses) else end_ns
        inside = [ns for ns in sends if press["realtime_ns"] <= ns < until]
        release = releases.get(press["index"])
        answered = len(inside) == 2 and release is not None and inside[1] >= release["realtime_ns"]
        rows.append(
            {
                "index": press["index"],
                "packets": len(inside),
                "latency_ms": (inside[0] - press["realtime_ns"]) / 1e6 if answered else None,
                "release_latency_ms": (
                    (inside[1] - release["realtime_ns"]) / 1e6 if answered and release is not None else None
                ),
            }
        )
    return rows


def match(events: list[dict], sends: list[int], end_ns: int) -> tuple[list[dict], str]:
    """Each measured press with its answer, and how the answers were found (ORDER or INTERVAL)."""
    rows = in_order(events, sends)
    if rows is not None:
        return rows, ORDER
    return by_interval(events, sends, end_ns), INTERVAL


def nearest_rank(values: list[float], share: float) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * share) - 1)]


def summarize(evidence: dict) -> dict:
    """`viewer.input_ms`, `viewer.input_missed` and the `viewer.input` record of one run.

    Raises PressFailed when the measure cannot speak for the viewer: the
    press did not reach the viewer's focused window, the wall clock moved,
    the capture saw none of the viewer's control stream, or a keyboard packet
    came that no press explains.
    """
    settings = evidence["settings"]
    if settings["hold_ms"] >= HOLD_LIMIT_MS:
        raise PressFailed(
            f"A hold of {settings['hold_ms']} ms lets the X server's auto-repeat add keys; "
            f"the limit is below {HOLD_LIMIT_MS} ms."
        )
    if evidence["press_exit"] != 0:
        raise PressFailed(
            f"The press program ended with {evidence['press_exit']}: "
            f"{evidence['press_errors'].strip() or 'no words'}"
        )
    log = press_log(evidence["press_log"])
    if not log["done"] or log["target"] is None:
        raise PressFailed("The press program did not finish its presses.")
    if log["target"]["owner_pid"] != evidence["viewer_pid"]:
        raise PressFailed("The pressed window does not belong to the viewer process.")
    events = log["events"]
    measured = [row for row in events if row["index"] >= 0]
    if len([row for row in measured if row["kind"] == "press"]) != settings["presses"]:
        raise PressFailed("The press program sent another number of presses than it was asked for.")
    unfocused = [row["index"] for row in events if not row["focused"]]
    if unfocused:
        raise PressFailed(
            f"The keyboard focus was not on the viewer's window at presses {sorted(set(unfocused))}: "
            "a key may have gone elsewhere."
        )
    offsets = [row["realtime_ns"] - row["monotonic_ns"] for row in events] + list(
        evidence["clock_offsets_ns"]
    )
    if max(offsets) - min(offsets) > CLOCK_STEP_LIMIT_NS:
        raise PressFailed("The wall clock was stepped during the presses; packet times are wall-clock.")
    packets = evidence["packets"]
    if not packets:
        raise PressFailed(
            "The capture saw no packet of the viewer's control stream: the capture or the port is wrong."
        )
    sends, unreadable = keyboard_sends(packets)
    if unreadable:
        raise PressFailed(f"{unreadable} datagrams to the control port are no ENet that this tool reads.")
    warmup = min(row["realtime_ns"] for row in events)
    first = min((row["realtime_ns"] for row in measured), default=evidence["capture_end_ns"])
    if any(ns < warmup for ns in sends) or len(sends) > len(events):
        raise PressFailed("A keyboard packet came that no press explains.")
    rows, attribution = match(events, sends, evidence["capture_end_ns"])
    latencies = [row["latency_ms"] for row in rows if row["latency_ms"] is not None]
    releases = [row["release_latency_ms"] for row in rows if row["release_latency_ms"] is not None]
    record = {
        "method": METHOD,
        "attribution": attribution,
        "presses": len(rows),
        "answered": len(latencies),
        "missed": len(rows) - len(latencies),
        "p50_ms": statistics.median(latencies) if latencies else None,
        "p95_ms": nearest_rank(latencies, 0.95) if latencies else None,
        "max_ms": max(latencies) if latencies else None,
        # A key-up is no press, and input_ms leaves it out; a late one is kept here.
        "release_p50_ms": statistics.median(releases) if releases else None,
        "release_max_ms": max(releases) if releases else None,
        "control_packets": len(packets),
        "keyboard_packets": len(sends),
        "warmup_packets": len([ns for ns in sends if warmup <= ns < first]),
        "target": log["target"],
        "settings": settings,
        "presses_detail": rows,
    }
    return {"input_ms": record["p50_ms"], "input_missed": record["missed"], "input": record}


def udp_port_filter(port: int) -> list[tuple[int, int, int, int]]:
    """Classic BPF for a cooked IPv4 packet socket: UDP to `port`, first fragments only."""
    return [
        (0x30, 0, 0, 9),  # ldb [9]: the IP protocol
        (0x15, 0, 6, socket.IPPROTO_UDP),
        (0x28, 0, 0, 6),  # ldh [6]: flags and fragment offset
        (0x45, 4, 0, 0x1FFF),  # a later fragment has no UDP header
        (0xB1, 0, 0, 0),  # ldxb 4*([0]&0xf): the IP header's length
        (0x48, 0, 0, 2),  # ldh [x+2]: the UDP destination port
        (0x15, 0, 1, port),
        (0x06, 0, 0, 0xFFFF),  # keep the packet
        (0x06, 0, 0, 0),  # drop it
    ]


def udp_datagram(packet: bytes, port: int) -> tuple[int, bytes] | None:
    """(source port, payload) of an IPv4 UDP datagram to `port`; None for anything else."""
    if len(packet) < 20 or packet[0] >> 4 != 4 or packet[9] != socket.IPPROTO_UDP:
        return None
    start = (packet[0] & 0x0F) * 4
    if len(packet) < start + 8 or struct.unpack_from(">H", packet, 6)[0] & 0x1FFF:
        return None
    source, destination, length = struct.unpack_from(">HHH", packet, start)
    if destination != port or length < 8:
        return None
    return source, packet[start + 8 : start + length]


class Capture:
    """The UDP packets to one port on this network namespace's loopback, with the kernel's times.

    The socket belongs to the network namespace of the thread that makes it;
    the agent makes it inside the pair's namespace, where loopback is the
    only interface. It reads, and sends nothing.
    """

    def __init__(self, port: int, network: str, interface: str = "lo") -> None:
        if not 0 < port < 1 << 16:
            raise ValueError(f"No UDP port: {port}.")
        # The socket takes the namespace of this thread: it must be the pair's.
        here = os_readlink("/proc/thread-self/ns/net")
        if here != network:
            raise PressFailed(
                f"The capture's thread is in {here}, not in the pair's network namespace "
                f"{network}; no packet socket opens there."
            )
        self.port = port
        self.packets: list[dict] = []
        self.socket = socket.socket(socket.AF_PACKET, socket.SOCK_DGRAM, socket.htons(ETH_P_IP))
        try:
            program = udp_port_filter(port)
            code = ctypes.create_string_buffer(b"".join(struct.pack("HBBI", *row) for row in program))
            self.socket.setsockopt(
                socket.SOL_SOCKET, SO_ATTACH_FILTER, struct.pack("HL", len(program), ctypes.addressof(code))
            )
            self.socket.setsockopt(socket.SOL_SOCKET, SO_TIMESTAMPNS, 1)
            self.socket.bind((interface, ETH_P_IP))
            self.socket.settimeout(0.05)
        except BaseException:
            self.socket.close()
            raise
        self.stopping = threading.Event()
        self.failure: BaseException | None = None
        self.thread = threading.Thread(target=self.read, daemon=True)
        self.thread.start()

    def read(self) -> None:
        try:
            while not self.stopping.is_set():
                try:
                    data, ancillary, _, _ = self.socket.recvmsg(1 << 16, socket.CMSG_SPACE(16))
                except socket.timeout:
                    continue
                stamp = next(
                    (
                        struct.unpack("qq", value[:16])
                        for level, kind, value in ancillary
                        if level == socket.SOL_SOCKET and kind == SO_TIMESTAMPNS
                    ),
                    None,
                )
                datagram = udp_datagram(data, self.port)
                if stamp is None or datagram is None:
                    continue  # A packet from before the filter, or without a kernel time.
                self.packets.append(
                    {"ns": stamp[0] * 1_000_000_000 + stamp[1], "source_port": datagram[0], "payload": datagram[1].hex()}
                )
        except BaseException as error:
            self.failure = error

    def close(self) -> tuple[list[dict], int]:
        """The packets, and the wall-clock time at which the capture stopped looking."""
        end = time.clock_gettime_ns(time.CLOCK_REALTIME)
        self.stopping.set()
        self.thread.join(timeout=5)
        self.socket.close()
        if self.failure is not None:
            raise PressFailed(f"The packet capture failed: {self.failure!r}")
        return list(self.packets), end
