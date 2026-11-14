"""Number 5: a press into the viewer's window, and the input packet that it sends."""

import fcntl
import json
import os
from pathlib import Path
import socket
import struct
import sys
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import cli  # noqa: E402
import input_measure  # noqa: E402
from input_measure import (  # noqa: E402
    KEYBOARD_CHANNEL,
    PressFailed,
    enet_commands,
    keyboard_sends,
    match,
    summarize,
)
from verdict import judge  # noqa: E402
import viewer  # noqa: E402

GENERIC_CHANNEL = 0x00


def command(kind: int, channel: int, sequence: int, data: bytes = b"") -> bytes:
    """One ENet command as moonlight-common-c's ENet writes it."""
    header = struct.pack(">BBH", kind, channel, sequence)
    number = kind & 0x0F  # The upper bits are flags, as ACKNOWLEDGE.
    if number == 6:  # SEND_RELIABLE
        return header + struct.pack(">H", len(data)) + data
    if number == 5:  # PING
        return header
    if number == 1:  # ACKNOWLEDGE
        return header + struct.pack(">HH", sequence, 0)
    raise ValueError(kind)


def datagram(*commands: bytes, sent_time: bool = True) -> bytes:
    peer = 0x0001 | (0x8000 if sent_time else 0)
    return struct.pack(">H", peer) + (struct.pack(">H", 1234) if sent_time else b"") + b"".join(commands)


def key(sequence: int) -> bytes:
    # 38 bytes: the encrypted keyboard packet's size does not matter here.
    return datagram(command(0x80 | 6, KEYBOARD_CHANNEL, sequence, bytes(38)))


def ping() -> bytes:
    return datagram(command(0x80 | 5, GENERIC_CHANNEL, 0))


MS = 1_000_000
OFFSET = 1_700_000_000_000_000_000  # wall clock minus monotonic clock


def press_log(times_ms: list[float], hold_ms: float = 30, focused: bool = True, owner: int = 4242) -> str:
    """press.c's output for a warm-up at 0 ms and a press at each time."""
    lines = ["target 4194305 %d 38" % owner]
    for index, at in [(-1, 0.0), *enumerate(times_ms)]:
        for kind, when in (("press", at), ("release", at + hold_ms)):
            monotonic = int(when * MS) + 5_000_000_000
            lines.append(f"{kind} {index} {monotonic + OFFSET} {monotonic} {1 if focused else 0}")
    return "\n".join(lines + ["done"]) + "\n"


def wall(at_ms: float) -> int:
    return int(at_ms * MS) + 5_000_000_000 + OFFSET


def evidence(packets: list[tuple[float, bytes]], times_ms: list[float], **changes) -> dict:
    row = {
        "settings": {"presses": len(times_ms), "hold_ms": 30, "gap_ms": 200, "jitter_ms": 100, "seed": 7},
        "press_exit": 0,
        "press_log": press_log(times_ms),
        "press_errors": "",
        "packets": [{"ns": wall(at), "source_port": 40000, "payload": data.hex()} for at, data in packets],
        "capture_end_ns": wall(times_ms[-1] + 700),
        "clock_offsets_ns": [OFFSET, OFFSET],
        "viewer_pid": 4242,
    }
    row.update(changes)
    return row


TIMES = [500.0, 760.0, 1010.0]


def answered_packets(delay_ms: float = 0.4) -> list[tuple[float, bytes]]:
    packets: list[tuple[float, bytes]] = [(100.0, ping())]
    for number, at in enumerate(TIMES):
        packets.append((at + delay_ms, key(2 * number + 1)))
        packets.append((at + 30 + delay_ms, key(2 * number + 2)))
    return packets


class EnetDatagrams(unittest.TestCase):
    def test_reads_each_command_with_its_channel_and_sequence(self):
        payload = datagram(command(1, GENERIC_CHANNEL, 9), command(0x80 | 6, KEYBOARD_CHANNEL, 3, b"x" * 40))
        self.assertEqual(enet_commands(payload), [(1, GENERIC_CHANNEL, 9), (6, KEYBOARD_CHANNEL, 3)])
        self.assertEqual(enet_commands(datagram(command(5, 0, 0), sent_time=False)), [(5, 0, 0)])

    def test_a_datagram_that_cannot_be_read_is_none_not_a_guess(self):
        whole = key(1)
        self.assertIsNone(enet_commands(whole[:-1]))  # data shorter than its length
        self.assertIsNone(enet_commands(struct.pack(">H", 0x4001) + whole[2:]))  # compressed
        self.assertIsNone(enet_commands(datagram(struct.pack(">BBH", 13, 0, 0))))  # no such command
        self.assertIsNone(enet_commands(b"\x80"))

    def test_a_resent_reliable_command_counts_once_and_other_channels_not_at_all(self):
        packets = [
            {"ns": 10, "payload": key(5).hex()},
            {"ns": 30, "payload": key(5).hex()},  # resent before an acknowledgement
            {"ns": 20, "payload": ping().hex()},
            {"ns": 40, "payload": key(6).hex()},
            {"ns": 50, "payload": "00"},
        ]
        self.assertEqual(keyboard_sends(packets), ([10, 40], 1))


class Matching(unittest.TestCase):
    def test_a_press_is_answered_by_one_key_down_and_one_key_up(self):
        log = input_measure.press_log(press_log(TIMES))
        sends = [wall(at) for at, data in answered_packets() if data != ping()]
        rows, attribution = match(log["events"], sends, wall(2000))
        self.assertEqual(attribution, "order")
        self.assertEqual([row["index"] for row in rows], [0, 1, 2])
        for row in rows:
            self.assertAlmostEqual(row["latency_ms"], 0.4, places=6)
            self.assertAlmostEqual(row["release_latency_ms"], 0.4, places=6)

    def test_a_late_key_up_that_leaves_with_the_next_key_down_keeps_its_own_delay(self):
        # aka, 2026-11-14 UTC: the key-up of one press left in the datagram of the next key-down.
        log = input_measure.press_log(press_log(TIMES))
        sends = [wall(TIMES[0] + 0.05), wall(TIMES[1] + 0.06), wall(TIMES[1] + 0.06),
                 wall(TIMES[1] + 30.05), wall(TIMES[2] + 0.05), wall(TIMES[2] + 30.05)]
        rows, attribution = match(log["events"], sends, wall(2000))
        self.assertEqual(attribution, "order")
        self.assertAlmostEqual(rows[1]["latency_ms"], 0.06, places=6)
        self.assertAlmostEqual(rows[0]["release_latency_ms"], TIMES[1] + 0.06 - TIMES[0] - 30, places=6)

    def test_the_warmups_two_packets_come_first_when_its_key_arrived(self):
        log = input_measure.press_log(press_log(TIMES))
        sends = [wall(0.1), wall(30.1)] + [wall(at) for at, data in answered_packets() if data != ping()]
        rows, attribution = match(log["events"], sends, wall(2000))
        self.assertEqual(attribution, "order")
        self.assertTrue(all(abs(row["latency_ms"] - 0.4) < 1e-6 for row in rows))

    def test_a_viewer_that_lost_a_key_is_judged_press_by_press(self):
        log = input_measure.press_log(press_log(TIMES))
        sends = [wall(at) for at, data in answered_packets() if data != ping()]
        del sends[3]  # the second key-up never left
        rows, attribution = match(log["events"], sends, wall(2000))
        self.assertEqual(attribution, "interval")
        self.assertEqual([row["packets"] for row in rows], [2, 1, 2])
        self.assertEqual([row["latency_ms"] is None for row in rows], [False, True, False])

    def test_no_packet_one_packet_or_an_early_second_packet_answers_nothing(self):
        log = input_measure.press_log(press_log(TIMES))
        sends = [wall(TIMES[1] + 1), wall(TIMES[2] + 1), wall(TIMES[2] + 2)]
        rows, attribution = match(log["events"], sends, wall(2000))
        self.assertEqual(attribution, "interval")
        self.assertEqual([row["packets"] for row in rows], [0, 1, 2])
        # The third press has two packets, but the second came before its release.
        self.assertEqual([row["latency_ms"] for row in rows], [None, None, None])

    def test_a_packet_before_its_own_key_is_no_answer_in_order(self):
        log = input_measure.press_log(press_log(TIMES))
        sends = [wall(at) for at, data in answered_packets() if data != ping()]
        sends[0] = wall(TIMES[0] - 50)  # before the first press: not its key-down
        rows, attribution = match(log["events"], sends, wall(2000))
        self.assertEqual(attribution, "interval")
        self.assertIsNone(rows[0]["latency_ms"])


class Summary(unittest.TestCase):
    def test_the_number_is_the_median_of_the_answered_presses(self):
        packets = answered_packets()
        packets[1] = (TIMES[0] + 2.0, packets[1][1])
        result = summarize(evidence(packets, TIMES))
        self.assertAlmostEqual(result["input_ms"], 0.4, places=6)
        self.assertEqual(result["input_missed"], 0)
        record = result["input"]
        self.assertEqual(record["method"], "x11-xtest-key")
        self.assertEqual(record["attribution"], "order")
        self.assertAlmostEqual(record["release_max_ms"], 0.4, places=6)
        self.assertEqual((record["presses"], record["answered"], record["missed"]), (3, 3, 0))
        self.assertAlmostEqual(record["max_ms"], 2.0, places=6)
        self.assertEqual(record["control_packets"], 7)
        self.assertEqual(record["keyboard_packets"], 6)
        self.assertEqual(record["target"]["owner_pid"], 4242)

    def test_a_viewer_that_sends_no_input_is_a_measurement_not_a_failure(self):
        # Korri's stage-1 viewer: the key reaches its window, and no input packet follows.
        result = summarize(evidence([(100.0, ping()), (900.0, ping())], TIMES))
        self.assertIsNone(result["input_ms"])
        self.assertEqual(result["input_missed"], 3)

    def test_a_warmup_answer_is_no_stray_packet(self):
        # Xvfb delivers the warm-up key that Xwayland drops; its two packets are no stray.
        times = [0.3, 30.3] + [at for at, data in answered_packets() if data != ping()]
        packets = [(at, key(sequence)) for sequence, at in enumerate(sorted(times), start=1)]
        result = summarize(evidence(packets, TIMES))
        self.assertEqual(result["input"]["warmup_packets"], 2)
        self.assertEqual(result["input_missed"], 0)

    def test_the_measure_refuses_to_speak_for_the_viewer_when_it_failed(self):
        good = answered_packets()
        cases = {
            "ended with 5": evidence(good, TIMES, press_exit=5, press_errors="PRESS no window"),
            "does not belong": evidence(good, TIMES, viewer_pid=1),
            "focus": evidence(good, TIMES, press_log=press_log(TIMES, focused=False)),
            "stepped": evidence(good, TIMES, clock_offsets_ns=[OFFSET, OFFSET + 2 * MS]),
            "no packet": evidence([], TIMES),
            "no press explains": evidence([(-50.0, key(99))] + good, TIMES),
            "explains": evidence(good + [(1500.0, key(90)), (1501.0, key(91)), (1502.0, key(92))], TIMES),
            "no ENet": evidence(good + [(900.0, b"\x80")], TIMES),
            "did not finish": evidence(good, TIMES, press_log=press_log(TIMES).replace("done\n", "")),
            "another number": evidence(good, TIMES, settings={"presses": 4}),
        }
        for words, row in cases.items():
            with self.subTest(words):
                with self.assertRaisesRegex(PressFailed, words):
                    summarize(row)


class RunRecord(unittest.TestCase):
    def test_a_run_with_no_press_says_why_for_both_numbers(self):
        numbers, notes = cli.input_numbers(None)
        self.assertEqual(numbers, {"input_ms": None, "input_missed": None})
        self.assertRegex(notes["input_ms"], "same-device PC pair")
        self.assertEqual(notes["input_ms"], notes["input_missed"])

    def test_a_viewer_that_answered_no_press_has_a_count_and_a_reason(self):
        pressed = summarize(evidence([(100.0, ping())], TIMES))
        numbers, notes = cli.input_numbers(pressed)
        self.assertEqual(numbers["input_missed"], 3)
        self.assertIsNone(numbers["input_ms"])
        self.assertEqual(numbers["input"]["presses"], 3)
        self.assertRegex(notes["input_ms"], "No press of 3")
        self.assertNotIn("input_missed", notes)
        numbers, notes = cli.input_numbers(summarize(evidence(answered_packets(), TIMES)))
        self.assertEqual(notes, {})


def run(input_ms: float | None, missed: int | None) -> dict:
    window = {
        "latency_ms": {"p50": 12.0, "p95": 14.0},
        "capture_misses": 0,
        "repeated_frames": 0,
        "late_frames": 0,
        "torn_frames": 0,
        "undecodable_frames": 0,
    }
    return {
        "discovered": {
            "valid": True,
            "viewer_ran": {"kind": "copied-product", "place": "fixture", "moonlight": "/viewer", "decode": "vaapi"},
            "display": {"mode": {"width": 640, "height": 480}},
        },
        "source_counter": {"windows": [window]},
        "viewer": {"startup_ms": 700.0, "input_ms": input_ms, "input_missed": missed},
    }


class InputVerdict(unittest.TestCase):
    def test_verdict_judges_the_time_and_the_missed_presses(self):
        moonlight = [run(0.3 + n / 100, 0) for n in range(5)]
        again = [run(0.31 + n / 100, 0) for n in range(5)]
        rows = {row["metric"]: row for row in judge(moonlight, again, 5)}
        self.assertEqual(rows["input_ms"]["verdict"], "equal")
        self.assertEqual(rows["input_missed"]["verdict"], "equal")
        self.assertEqual(rows["input_missed"]["number"], "5 input")
        slow = [run(5.0 + n, 0) for n in range(5)]
        self.assertEqual({row["metric"]: row for row in judge(moonlight, slow, 5)}["input_ms"]["verdict"], "worse")

    def test_a_viewer_that_sends_no_input_is_worse_not_hidden(self):
        moonlight = [run(0.3, 0) for _ in range(5)]
        silent = [run(None, 20) for _ in range(5)]
        rows = {row["metric"]: row for row in judge(moonlight, silent, 5)}
        self.assertEqual(rows["input_missed"]["verdict"], "worse")
        self.assertEqual(rows["input_ms"]["verdict"], "not measured")


def loopback_up() -> None:
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as control:
        request = struct.pack("16sH22x", b"lo", 0)
        flags = struct.unpack("16sH22x", fcntl.ioctl(control, 0x8913, request))[1]
        fcntl.ioctl(control, 0x8914, struct.pack("16sH22x", b"lo", flags | 1))


def in_private_network(work) -> dict:
    """Run `work` in a child with its own user and network namespace, as an unprivileged test can."""
    read, write = os.pipe()
    child = os.fork()
    if child == 0:
        os.close(read)
        try:
            uid, gid = os.getuid(), os.getgid()
            os.unshare(os.CLONE_NEWUSER | os.CLONE_NEWNET)
            Path("/proc/self/setgroups").write_text("deny")
            Path("/proc/self/uid_map").write_text(f"0 {uid} 1")
            Path("/proc/self/gid_map").write_text(f"0 {gid} 1")
            loopback_up()
            result = {"ok": work()}
        except BaseException as error:  # noqa: BLE001 - the parent reports it
            result = {"error": repr(error)}
        os.write(write, json.dumps(result).encode())
        os._exit(0)
    os.close(write)
    data = b""
    while chunk := os.read(read, 1 << 16):
        data += chunk
    os.close(read)
    os.waitpid(child, 0)
    result = json.loads(data)
    if "error" in result:
        raise AssertionError(result["error"])
    return result["ok"]


def capture_on_loopback() -> dict:
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as control_port, socket.socket(
        socket.AF_INET, socket.SOCK_DGRAM
    ) as other_port:
        loopback = str(socket.inet_ntoa(struct.pack(">I", socket.INADDR_LOOPBACK)))
        control_port.bind((loopback, 0))
        other_port.bind((loopback, 0))
        port = control_port.getsockname()[1]
        capture = input_measure.Capture(port)
        sent = []
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as viewer_socket:
            for number in range(3):
                before = time.clock_gettime_ns(time.CLOCK_REALTIME)
                viewer_socket.sendto(key(number + 1), (loopback, port))
                after = time.clock_gettime_ns(time.CLOCK_REALTIME)
                sent.append([before, after])
                viewer_socket.sendto(b"not to the control port", (loopback, other_port.getsockname()[1]))
                time.sleep(0.01)
            source = viewer_socket.getsockname()[1]
        time.sleep(0.1)
        packets, end = capture.close()
        return {"packets": packets, "sent": sent, "end": end, "source": source}


class LoopbackCapture(unittest.TestCase):
    def test_the_kernel_stamps_each_datagram_to_the_control_port_and_only_those(self):
        seen = in_private_network(capture_on_loopback)
        packets = seen["packets"]
        self.assertEqual([bytes.fromhex(row["payload"]) for row in packets], [key(n) for n in (1, 2, 3)])
        self.assertTrue(all(row["source_port"] == seen["source"] for row in packets))
        for row, (before, after) in zip(packets, seen["sent"]):
            self.assertGreaterEqual(row["ns"], before)
            self.assertLessEqual(row["ns"], after)
        self.assertGreater(seen["end"], packets[-1]["ns"])

    def test_a_port_that_is_no_udp_port_is_refused_before_any_socket(self):
        with self.assertRaisesRegex(ValueError, "No UDP port"):
            input_measure.Capture(0)


class AgentRoute(unittest.TestCase):
    def test_the_agent_presses_only_inside_the_pair_namespace_with_a_running_viewer(self):
        session = object.__new__(viewer.Session)
        session.x_namespace = False
        session.processes = {}
        with self.assertRaisesRegex(RuntimeError, "same-device PC pair"):
            session.press({"press": "/bin/false", "control_port": 1})
        session.x_namespace = True
        with self.assertRaisesRegex(RuntimeError, "running viewer"):
            session.press({"press": "/bin/false", "control_port": 1})


if __name__ == "__main__":
    unittest.main()
