"""--net and --require-encryption: a real network's conditions on the private pair alone.

The real-namespace tests run in a child with its own user, network and mount
namespaces, as an unprivileged test can. The child's first network namespace
stands for the product's; the pair's namespace is made inside it by the
agents' own `PairNamespace`. The tests derivation names iproute2 in
CADENCE_IPROUTE2.
"""

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
from cli import Refusal, encryption_record, net_refusal, parser  # noqa: E402
from fixture_namespace import PairNamespace  # noqa: E402
import net_condition  # noqa: E402
from net_condition import condition, recovery  # noqa: E402
import sender  # noqa: E402
from test_private_sender import Machine  # noqa: E402
from verdict import conditions, judge, net_lines  # noqa: E402

LOOPBACK = socket.inet_ntoa(struct.pack(">I", socket.INADDR_LOOPBACK))
UDP_SEGMENT = 103  # linux/udp.h


def iproute2() -> str:
    path = os.environ.get("CADENCE_IPROUTE2")
    if not path:
        raise AssertionError("CADENCE_IPROUTE2 is not set: run these tests through the tests output of default.nix.")
    return path


def loopback_up() -> None:
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as control:
        request = struct.pack("16sH22x", b"lo", 0)
        flags = struct.unpack("16sH22x", fcntl.ioctl(control, 0x8913, request))[1]
        fcntl.ioctl(control, 0x8914, struct.pack("16sH22x", b"lo", flags | 1))


def in_child(work) -> object:
    """Run `work(pair)` in a child: its own user namespace, a 'product' network, and a real pair namespace in it.

    `pair` is None when `work` wants to stay in the 'product' network.
    """
    read, write = os.pipe()
    child = os.fork()
    if child == 0:
        os.close(read)
        try:
            uid, gid = os.getuid(), os.getgid()
            os.unshare(os.CLONE_NEWUSER | os.CLONE_NEWNET | os.CLONE_NEWNS)
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


def pair_with(spec: str) -> tuple[PairNamespace, dict]:
    pair = PairNamespace()
    pair.create()
    return pair, net_condition.apply(pair, condition(spec)["netem"], iproute2())


def batches_through_the_loopback(count: int, segments: int) -> dict[int, int]:
    """Send UDP GSO batches as Sunshine sends a frame's packets; count the datagrams of each batch that arrive."""
    arrived: dict[int, int] = {}
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as receiver, socket.socket(
        socket.AF_INET, socket.SOCK_DGRAM
    ) as sender_socket:
        receiver.bind((LOOPBACK, 0))
        receiver.setblocking(False)
        size = 1400
        sender_socket.setsockopt(socket.SOL_UDP, UDP_SEGMENT, size)

        def drain() -> None:
            while True:
                try:
                    data = receiver.recv(1 << 16)
                except BlockingIOError:
                    return
                batch = struct.unpack_from(">I", data)[0]
                arrived[batch] = arrived.get(batch, 0) + 1

        for batch in range(count):
            payload = b"".join(struct.pack(">II", batch, n).ljust(size, b"\0") for n in range(segments))
            sender_socket.sendto(payload, receiver.getsockname())
            time.sleep(0.002)
            drain()
        time.sleep(0.2)
        drain()
    return arrived


class Condition(unittest.TestCase):
    """The words of --net are netem's own, a small checked part of them."""

    def test_the_four_conditions_in_netems_words(self):
        self.assertEqual(
            condition("loss random 1%"),
            {"netem": "loss random 1%", "delay_ms": 0.0, "mean_loss_percent": 1.0, "mean_burst_packets": None, "reorder_percent": 0.0},
        )
        burst = condition("loss gemodel 1.3158% 25%")
        self.assertEqual(burst["netem"], "loss gemodel 1.3158% 25%")
        # Gilbert's model: p into the bad state, r out of it, every packet in it lost.
        self.assertAlmostEqual(burst["mean_loss_percent"], 5.0, places=3)
        self.assertEqual(burst["mean_burst_packets"], 4.0)
        self.assertEqual(condition("delay 30ms")["delay_ms"], 30.0)
        reorder = condition("reorder 25% delay 10ms")
        # One order of the words, the one that the record and tc get.
        self.assertEqual(reorder["netem"], "delay 10ms reorder 25%")
        self.assertEqual(reorder["reorder_percent"], 25.0)
        self.assertEqual(condition("  delay 30.0ms   loss random 0.5% ")["netem"], "delay 30ms loss random 0.5%")

    def test_words_outside_the_part_are_refused_and_named(self):
        for spec, words in (
            ("", "names no condition"),
            ("rate 1mbit", "rate.*delay, loss, reorder"),
            ("delay 30", "delay.*milliseconds"),
            ("delay 30ms 5ms", "5ms"),
            ("delay 0ms", "delay.*above 0"),
            ("delay 1001ms", "1000 ms"),
            ("loss 1%", "loss random.*loss gemodel"),
            ("loss random 1% 25%", "25%"),
            ("loss random 0%", "between 0 and 100"),
            ("loss random 100%", "between 0 and 100"),
            ("loss random 1", "percent"),
            ("loss gemodel 1%", "gemodel.*p.*r"),
            ("loss random 1% loss random 2%", "once"),
            ("delay 5ms delay 6ms", "once"),
            ("reorder 25%", "reorder.*delay"),
        ):
            with self.subTest(spec=spec):
                with self.assertRaisesRegex(ValueError, words):
                    condition(spec)

    def test_every_command_names_the_loopback_and_nothing_else(self):
        ip, tc = "/iproute2/bin/ip", "/iproute2/bin/tc"
        base = [
            [ip, "link", "set", "dev", "lo", "mtu", "1500"],
            [tc, "qdisc", "add", "dev", "lo", "root", "handle", "1:", "prio", "bands", "2",
             "priomap", *["1"] * 16],
            [tc, "filter", "add", "dev", "lo", "parent", "1:", "protocol", "ip", "prio", "1",
             "u32", "match", "ip", "protocol", "17", "0xff", "flowid", "1:1"],
            [tc, "filter", "add", "dev", "lo", "parent", "1:", "protocol", "ipv6", "prio", "2",
             "u32", "match", "ip6", "protocol", "17", "0xff", "flowid", "1:1"],
            [tc, "qdisc", "add", "dev", "lo", "parent", "1:1", "handle", "10:", "tbf",
             "rate", "100gbit", "burst", "1514", "limit", "100000"],
        ]
        self.assertEqual(
            net_condition.commands("delay 30ms loss random 1%", "/iproute2"),
            [
                *base,
                [tc, "qdisc", "add", "dev", "lo", "parent", "10:1", "handle", "11:",
                 "netem", "limit", "100000", "delay", "30ms", "loss", "random", "1%"],
                # TCP takes the delay and nothing else.
                [tc, "qdisc", "add", "dev", "lo", "parent", "1:2", "handle", "20:",
                 "netem", "limit", "100000", "delay", "30ms"],
            ],
        )
        # With no delay, TCP keeps the band's own queue: no netem at all.
        self.assertEqual(
            net_condition.commands("loss random 1%", "/iproute2"),
            [*base, [tc, "qdisc", "add", "dev", "lo", "parent", "10:1", "handle", "11:",
                     "netem", "limit", "100000", "loss", "random", "1%"]],
        )
        for command in net_condition.commands("delay 10ms reorder 25%", "/iproute2"):
            self.assertEqual(command[command.index("dev") + 1], "lo")


class CoreLog(unittest.TestCase):
    def test_the_cores_own_lines_of_loss_and_recovery_are_counted(self):
        log = "\n".join(
            [
                "Unrecoverable frame 812: 14+2=16 received < 17 needed",
                "Unrecoverable frame 812 (block 1 of 2): 10+0=10 received < 12 needed",
                "Unrecoverable frame 900: lost FEC blocks 1 to 2",
                "Network dropped 1 frame (frame 811)",
                "Network dropped 3 frames (frames 896 to 898)",
                "Sending RFI request for unrecoverable frame 812",
                "Invalidate reference frame request sent (812 to 812)",
                "IDR frame request sent",
                "Network dropped audio data (expected 70, but received 72)",
                "Unable to recover audio data block 64 to 67 (2+0=2 received < 4 needed)",
                "Connection is poor",
            ]
        )
        self.assertEqual(
            recovery(log),
            {
                "unrecoverable_frames": 2,
                "network_dropped_frames": 4,
                "rfi_requests": 1,
                "idr_requests": 1,
                "audio_dropped": 1,
                "audio_unrecovered_blocks": 1,
            },
        )
        self.assertEqual(set(recovery("").values()), {0})


class Encryption(unittest.TestCase):
    def test_required_encryption_is_sunshines_mandatory_mode_for_every_address(self):
        def values(**extra) -> dict:
            text = sender.configuration(Path("/tmp/run"), 40123, "/dev/dri/renderD1", **extra)
            return dict(line.split(" = ", 1) for line in text.splitlines())

        required = values(encryption_required=True)
        self.assertEqual(required["lan_encryption_mode"], "2")
        self.assertEqual(required["wan_encryption_mode"], "2")
        # The default is today's file: Sunshine's own defaults decide.
        default = values()
        self.assertNotIn("lan_encryption_mode", default)
        self.assertNotIn("wan_encryption_mode", default)
        self.assertEqual({**required}, {**default, "lan_encryption_mode": "2", "wan_encryption_mode": "2"})

    def test_a_run_proves_its_encryption_from_sunshines_own_log(self):
        # Lines from aka, 2026-11-15 UTC: the locating start and the measured start.
        journal = (
            "[2026-11-15 00:11:25.797]: Info: Video encryption enabled\n"
            "[2026-11-15 00:11:30.001]: Info: CLIENT CONNECTED\n"
            "[2026-11-15 00:11:35.227]: Info: Video encryption enabled\n"
        )
        required = {"required": True, "sunshine": {"lan_encryption_mode": 2, "wan_encryption_mode": 2}}
        record = encryption_record(required, "Starting RTSP handshake...\n", journal)
        self.assertEqual(len(record["sunshine_log"]), 2)
        self.assertEqual(record["viewer_log"], [])
        self.assertTrue(record["required"])
        opted_out = encryption_record(
            required, "Enabling video encryption by host request despite client opt-out. Performance may suffer!\n", journal
        )
        self.assertEqual(len(opted_out["viewer_log"]), 1)
        with self.assertRaisesRegex(Refusal, "Video encryption enabled.*journal.log"):
            encryption_record(required, "", "[2026-11-15 00:11:30.001]: Info: CLIENT CONNECTED\n")


def run(net: dict | None = None, encryption: dict | None = None) -> dict:
    window = {
        "latency_ms": {"p50": 20.0, "p95": 22.0}, "capture_misses": 0, "repeated_frames": 0,
        "late_frames": 0, "torn_frames": 0, "undecodable_frames": 0,
    }
    facts: dict = {
        "valid": True, "model": "host", "viewer": {"model": "host"}, "max_capture_gaps": 0,
        "viewer_ran": {"kind": "stand-in", "place": "fixture", "moonlight": "/viewer", "decode": "vaapi"},
    }
    if net is not None:
        facts["net"] = net
    if encryption is not None:
        facts["encryption"] = encryption
    return {"discovered": facts, "source_counter": {"windows": [window]}, "viewer": {"startup_ms": 900.0}}


class Verdict(unittest.TestCase):
    def test_todays_default_runs_and_the_landed_baselines_keep_their_conditions(self):
        # A landed baseline has neither fact, and neither has a run without the
        # options: the baselines are measurement data and not in this source.
        landed = [run() for _ in range(5)]
        for record in landed:
            self.assertNotIn("net", record["discovered"])
            self.assertNotIn("encryption", record["discovered"])
            self.assertIsNone(conditions(record)["net"])
            self.assertFalse(conditions(record)["encryption"])
        self.assertEqual(len(judge(landed, [run() for _ in range(5)], 5)), 16)

    def test_a_network_condition_keeps_two_results_apart(self):
        lossy = condition("loss random 1%")
        with self.assertRaisesRegex(ValueError, "net") as refused:
            judge([run() for _ in range(5)], [run(lossy) for _ in range(5)], 5)
        self.assertIn("netem", str(refused.exception))
        with self.assertRaisesRegex(ValueError, "net"):
            judge([run(condition("delay 30ms")) for _ in range(5)], [run(lossy) for _ in range(5)], 5)
        self.assertEqual(len(judge([run(lossy) for _ in range(5)], [run(dict(lossy)) for _ in range(5)], 5)), 16)

    def test_required_encryption_keeps_two_results_apart(self):
        required = {"required": True}
        with self.assertRaisesRegex(ValueError, "encryption") as refused:
            judge([run() for _ in range(5)], [run(encryption=required) for _ in range(5)], 5)
        self.assertIn("encrypt", str(refused.exception))
        self.assertEqual(len(judge([run(encryption=required) for _ in range(5)], [run(encryption=required) for _ in range(5)], 5)), 16)

    def test_the_verdict_names_the_condition_and_what_it_did(self):
        def lossy(dropped: int, unrecoverable: int) -> dict:
            return run(dict(condition("loss random 1%"), measured_start={"passed": 9000, "dropped": dropped},
                            core_log={"unrecoverable_frames": unrecoverable, "rfi_requests": unrecoverable}))

        lines = net_lines([lossy(90, 0), lossy(110, 1)], [lossy(95, 0), lossy(100, 0)])
        self.assertEqual(len(lines), 2)
        self.assertIn("loss random 1%", lines[0])
        self.assertIn("dropped 90..110", lines[0])
        self.assertIn("unrecoverable frames 0..1", lines[0])
        self.assertEqual(net_lines([run()], [run()]), [])


class Options(unittest.TestCase):
    def test_the_options_parse(self):
        args = parser().parse_args(
            ["view", "--sender", "a", "--viewer", "b", "--net", "loss random 1%", "--require-encryption"]
        )
        self.assertEqual(args.net, "loss random 1%")
        self.assertTrue(args.require_encryption)
        default = parser().parse_args(["view", "--sender", "a", "--viewer", "b"])
        self.assertIsNone(default.net)
        self.assertFalse(default.require_encryption)

    def view_refuses_before_any_device(self, *extra: str, words: str) -> None:
        asked: list = []

        def no_device(*command, **options):
            asked.append(command)
            raise AssertionError("a device was asked")

        for name in ("execute", "tools"):
            previous = getattr(cli, name)
            setattr(cli, name, no_device)
            self.addCleanup(setattr, cli, name, previous)
        previous_run = cli.subprocess.run
        cli.subprocess.run = no_device
        self.addCleanup(setattr, cli.subprocess, "run", previous_run)
        args = parser().parse_args(["view", "--sender", "root@aka", "--viewer", "root@aka", *extra])
        with self.assertRaisesRegex((Refusal, ValueError), words):
            cli.view(args)
        self.assertEqual(asked, [])

    def test_view_refuses_bad_words_and_a_pair_without_its_fixture_before_any_device(self):
        self.view_refuses_before_any_device("--sender-fixture", "--net", "loss 1%", words="loss random")
        self.view_refuses_before_any_device("--net", "delay 30ms", words="--net.*--sender-fixture")
        self.view_refuses_before_any_device("--require-encryption", words="--require-encryption.*--sender-fixture")

    def test_a_condition_needs_the_pairs_own_network_namespace(self):
        lossy = condition("loss random 1%")
        self.assertRegex(net_refusal(lossy, private_x=False), "own network namespace.*no real interface")
        self.assertIsNone(net_refusal(lossy, private_x=True))
        self.assertIsNone(net_refusal(None, private_x=False))


class SenderRefusal(Machine):
    def test_the_sender_applies_no_condition_without_the_pair_namespace(self):
        session = sender.Session()
        self.addCleanup(session.close)
        with self.assertRaisesRegex(RuntimeError, "--net.*pair's own network namespace"):
            session.handle(
                {
                    "operation": "prepare", "sway": str(self.peers / "sway"),
                    "sunshine": str(self.peers / "sunshine"), "width": 640, "height": 480,
                    "fps": 60.024, "net": "delay 30ms", "iproute2": "/iproute2",
                }
            )
        self.assertIsNone(session.root)
        self.assertFalse((self.peers / "arguments.log").exists())

    def test_the_net_request_needs_a_condition(self):
        session = sender.Session()
        self.addCleanup(session.close)
        with self.assertRaisesRegex(RuntimeError, "not prepared"):
            session.handle({"operation": "net"})


class PairLoopback(unittest.TestCase):
    """netem on the pair's own loopback, in real namespaces."""

    def test_loss_falls_on_datagrams_not_on_a_frames_whole_batch(self):
        def work() -> dict:
            _, applied = pair_with("loss random 50%")
            arrived = batches_through_the_loopback(100, 20)
            counted = net_condition.counters(iproute2())
            return {"arrived": arrived, "counted": counted, "applied": applied}

        seen = in_child(work)
        arrived = seen["arrived"]
        # A batch is one sendmsg with UDP_SEGMENT, as Sunshine sends a frame's
        # packets. netem alone drops a whole batch or none of it.
        partial = [count for count in arrived.values() if 0 < count < 20]
        self.assertGreater(len(partial), 80, arrived)
        total = sum(arrived.values())
        self.assertTrue(700 < total < 1300, total)
        self.assertEqual(seen["counted"]["passed"], total)
        self.assertEqual(seen["counted"]["passed"] + seen["counted"]["dropped"], 2000)
        self.assertEqual(sorted(row["kind"] for row in seen["applied"]["qdiscs"]), ["netem", "prio", "tbf"])
        self.assertEqual(seen["applied"]["mtu"], 1500)

    def test_delay_holds_each_direction(self):
        def work() -> float:
            pair_with("delay 20ms")
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as a, socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as b:
                a.bind((LOOPBACK, 0))
                b.bind((LOOPBACK, 0))
                began = time.monotonic()
                a.sendto(b"ping", b.getsockname())
                data, address = b.recvfrom(64)
                b.sendto(data, address)
                a.recv(64)
                return (time.monotonic() - began) * 1000

        round_trip = in_child(work)
        self.assertGreaterEqual(round_trip, 40.0)
        self.assertLess(round_trip, 200.0)

    def test_the_longest_delay_overflows_no_queue(self):
        # netem holds every packet of the delay. Its own limit is 1000
        # packets: about 0.8 s of a 10 Mbit/s stream of 1024-byte packets, so
        # a long delay would drop by overflow what no condition asked for.
        def work() -> dict:
            pair_with(f"delay {int(net_condition.LIMIT_MS)}ms")
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as receiver, socket.socket(
                socket.AF_INET, socket.SOCK_DGRAM
            ) as sender_socket:
                receiver.bind((LOOPBACK, 0))
                began = time.monotonic()
                for n in range(3000):
                    sender_socket.sendto(struct.pack(">I", n), receiver.getsockname())
                sent_in = time.monotonic() - began
                held = net_condition.counters(iproute2())
                time.sleep(net_condition.LIMIT_MS / 1000 + 0.5)
                counted = net_condition.counters(iproute2())
            return {"sent_in": sent_in, "held": held, "counted": counted}

        seen = in_child(work)
        # All 3000 were in netem's queue at once, and none was dropped.
        self.assertLess(seen["sent_in"], net_condition.LIMIT_MS / 1000)
        self.assertEqual(seen["held"], {"passed": 0, "dropped": 0})
        self.assertEqual(seen["counted"], {"passed": 3000, "dropped": 0})

    def test_tcp_takes_the_delay_and_no_loss(self):
        # netem's loss models count packets, not time. On the few packets of a
        # TCP handshake a loss is a stall of a second or more: on aka, under a
        # 5% burst loss on every packet, one RTSP handshake took 13 s, Sunshine
        # ended the session for want of a ping, and the viewer hung.
        def work() -> dict:
            pair_with("delay 20ms loss random 50%")
            with socket.socket() as server:
                server.bind((LOOPBACK, 0))
                server.listen(8)
                # A lost packet must fail the test, not hang it.
                server.settimeout(10)
                times = []
                for _ in range(10):
                    began = time.monotonic()
                    with socket.create_connection(server.getsockname(), timeout=10) as client:
                        accepted, _ = server.accept()
                        with accepted:
                            client.sendall(b"OPTIONS rtsp://x RTSP/1.0\r\n\r\n")
                            accepted.settimeout(10)
                            accepted.recv(1024)
                            accepted.sendall(b"RTSP/1.0 200 OK\r\n\r\n")
                            client.recv(1024)
                    times.append((time.monotonic() - began) * 1000)
            counted = net_condition.counters(iproute2())
            return {"times": times, "counted": counted}

        seen = in_child(work)
        # A connect and one request and answer: two round trips of 40 ms, no resend.
        self.assertTrue(all(70.0 <= ms < 400.0 for ms in seen["times"]), seen["times"])
        # netem on the UDP band saw none of it.
        self.assertEqual(seen["counted"], {"passed": 0, "dropped": 0})

    def test_the_product_network_is_untouched_and_the_record_proves_it(self):
        def work() -> dict:
            product = os.readlink("/proc/self/ns/net")
            pair, applied = pair_with("delay 30ms loss gemodel 1.3158% 25% reorder 25%")
            after = net_condition.product_network(pair, iproute2())
            return {"product": product, "applied": applied, "after": after, "pair": os.readlink("/proc/self/ns/net")}

        seen = in_child(work)
        applied, after = seen["applied"], seen["after"]
        self.assertEqual(applied["namespace"], seen["pair"])
        self.assertEqual(applied["product_namespace"], seen["product"])
        self.assertNotEqual(applied["namespace"], applied["product_namespace"])
        # The pair's namespace has one interface: its own loopback.
        self.assertEqual(applied["interfaces"], ["lo"])
        self.assertEqual(after["namespace"], seen["product"])
        self.assertEqual(after["mtu"], {"lo": 65536})
        self.assertEqual([row["kind"] for row in after["qdiscs"]], ["noqueue"])
        self.assertEqual(applied["product_before"], after)
        self.assertEqual(net_condition.untouched(applied["product_before"], after), [])

    def test_a_netem_or_mtu_change_in_the_product_is_named(self):
        before = {"namespace": "net:[1]", "mtu": {"lo": 65536, "eth0": 1500}, "qdiscs": [{"dev": "lo", "kind": "noqueue", "handle": "0:", "parent": None}]}
        changed = {
            "namespace": "net:[1]",
            "mtu": {"lo": 1500, "eth0": 1500, "veth9": 1500},
            "qdiscs": [*before["qdiscs"], {"dev": "eth0", "kind": "netem", "handle": "10:", "parent": "1:1"}],
        }
        found = net_condition.untouched(before, changed)
        self.assertEqual(len(found), 2)
        self.assertRegex(found[0], "lo.*65536.*1500")
        self.assertRegex(found[1], "netem.*eth0")
        # An interface that appears, as a container's, is no change of the product's network.
        self.assertEqual(net_condition.untouched(before, {**before, "mtu": {**before["mtu"], "veth9": 1500}}), [])

    def test_nothing_is_applied_outside_a_pair_namespace(self):
        def work() -> dict:
            found = {}
            try:
                net_condition.apply(PairNamespace(), "delay 30ms", iproute2())
            except RuntimeError as error:
                found["inactive"] = str(error)
            here = PairNamespace()
            # An agent whose 'pair' is the namespace it already stands in: the product's.
            here.original = {"net": os.open("/proc/self/ns/net", os.O_RDONLY | os.O_CLOEXEC)}
            here.active = True
            try:
                net_condition.apply(here, "delay 30ms", iproute2())
            except RuntimeError as error:
                found["product"] = str(error)
            found["qdiscs"] = [row["kind"] for row in net_condition.snapshot(iproute2())["qdiscs"]]
            found["mtu"] = net_condition.snapshot(iproute2())["mtu"]
            return found

        found = in_child(work)
        self.assertRegex(found["inactive"], "no pair namespace")
        self.assertRegex(found["product"], "product network namespace")
        self.assertEqual(found["qdiscs"], ["noqueue"])
        self.assertEqual(found["mtu"], {"lo": 65536})


if __name__ == "__main__":
    unittest.main()
