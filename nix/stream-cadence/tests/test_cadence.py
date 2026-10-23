import contextlib
import io
import json
import os
import shlex
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from bar import Clean, Torn, Unreadable, paint, read, search
from cli import compare, parser, ssh_target, summary, tools
from decode import decode
from discovery import (
    credentials,
    encoder_probe,
    environment,
    output,
    properties,
    session_log,
    tcp_ports,
)
from score import InvalidMeasurement, bridge, score

FIXTURES = Path(__file__).parent / "fixtures"


class BarcodeTests(unittest.TestCase):
    def test_sync_search_not_fixed_crop(self):
        image = paint(640, 480, 0xACAB)
        moved = np.roll(np.roll(image, 37, axis=0), -11, axis=1)
        self.assertEqual(read(moved, search(moved)), Clean(0xACAB))
        location = search(image)
        image[int(location.y + 3 * location.cell)] = 128
        self.assertIsInstance(read(image, location), Unreadable)

    def test_lossy_h264_roundtrip_all_resolutions(self):
        for width, height in [(640, 480), (1280, 720), (1920, 1080)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                ids = [1, 2, 255, 65535, 0xABCDE, 0xFFFFFF]
                render = os.environ.get("CADENCE_BAR_RENDER")
                if render:
                    frames = [
                        subprocess.check_output(
                            [render, str(width), str(height), str(ident)]
                        )
                        for ident in ids
                    ]
                else:
                    frames = [paint(width, height, ident).tobytes() for ident in ids]
                video = root / "test.h264"
                subprocess.run(
                    [
                        "ffmpeg",
                        "-v",
                        "error",
                        "-f",
                        "rawvideo",
                        "-pixel_format",
                        "gray",
                        "-video_size",
                        f"{width}x{height}",
                        "-framerate",
                        "30",
                        "-i",
                        "-",
                        "-c:v",
                        "libx264",
                        "-preset",
                        "fast",
                        "-crf",
                        "32",
                        "-pix_fmt",
                        "yuv420p",
                        str(video),
                    ],
                    input=b"".join(frames),
                    check=True,
                    capture_output=True,
                )
                self.assertEqual(
                    decode(video, root / "decode.log"), [Clean(ident) for ident in ids]
                )

    def test_independently_valid_rows_with_different_ids_are_torn(self):
        image = paint(640, 480, 70)
        other = paint(640, 480, 71)
        location = search(image)
        start = int(location.y + 3 * location.cell)
        image[start : start + int(2 * location.cell)] = other[
            start : start + int(2 * location.cell)
        ]
        self.assertEqual(read(image, location), Torn(70, 71))
        self.assertEqual(read(image, search(image)), Torn(70, 71))

    def test_crc_broken_row_is_unreadable_not_torn(self):
        image = paint(640, 480, 70)
        location = search(image)
        y = int(location.y + 3 * location.cell)
        x = int(location.x + 35 * location.cell)
        image[y : y + int(2 * location.cell), x : x + int(location.cell)] = (
            255 - image[y : y + int(2 * location.cell), x : x + int(location.cell)]
        )
        frame = read(image, location)
        self.assertIsInstance(frame, Unreadable)
        self.assertIn("CRC", frame.reason)
        ids, rows, source = measurement(30)
        ids[70] = frame
        with self.assertRaisesRegex(ValueError, "unreadable scored ID"):
            score(
                ids,
                rows,
                source,
                lambda ns: ns,
                warmup=2,
                seconds=4,
                window=2,
                fps=30,
                refresh=30,
            )

    def test_no_sync_fails(self):
        with self.assertRaisesRegex(ValueError, "sync pattern not found"):
            search(np.full((240, 320), 96, dtype=np.uint8))


def measurement(rate, drop=(), discard=()):
    # Two warm-up seconds, two scored windows and a tail, on the receive clock.
    ids, rows, source = [], [], []
    for ident in range(1, rate * 7 + 1):
        ns = round((ident - 1) * 1e9 / rate)
        source.append(f"SUBMIT id={ident} submit_ns={ns}")
        if ident in discard:
            source.append(f"DISCARD id={ident}")
        else:
            source.append(
                f"PRESENT id={ident} submit_ns={ns} present_ns={ns} sequence={ident}"
            )
            if ident not in drop:
                ids.append(Clean(ident))
                rows.append({"index": len(rows), "receive_ms": ns // 1_000_000 + 5})
    return ids, rows, "\n".join(source)


class ScoringTests(unittest.TestCase):
    def test_windows_against_each_display_ceiling(self):
        for rate in [30, 60, 120]:
            with self.subTest(rate=rate):
                ids, rows, source = measurement(rate)
                report = score(
                    ids,
                    rows,
                    source,
                    lambda ns: ns,
                    warmup=2,
                    seconds=4,
                    window=2,
                    fps=rate,
                    refresh=rate,
                )
                self.assertEqual(report["raw_warmup"]["new_frames"], rate * 2)
                self.assertEqual(
                    [row["new_frames"] for row in report["windows"]], [rate * 2] * 2
                )
                self.assertEqual(
                    [row["expected_new_frames"] for row in report["windows"]],
                    [rate * 2] * 2,
                )
                self.assertEqual(
                    [row["capture_misses"] for row in report["windows"]], [0, 0]
                )
                self.assertGreaterEqual(report["windows"][0]["latency_ms"]["p50"], 4)
                self.assertLessEqual(report["windows"][0]["latency_ms"]["p50"], 5)

    def test_stopped_presentations_with_repeated_last_id_are_invalid(self):
        ids, rows, source = measurement(30)
        source = "\n".join(
            line
            for line in source.splitlines()
            if int(line.split("id=")[1].split()[0]) <= 91
        )
        ids[91:] = [Clean(91)] * (len(ids) - 91)
        with self.assertRaisesRegex(
            InvalidMeasurement, "presentation feedback does not cover scored windows"
        ) as raised:
            score(
                ids,
                rows,
                source,
                lambda ns: ns,
                warmup=2,
                seconds=4,
                window=2,
                fps=30,
                refresh=30,
            )
        self.assertEqual(raised.exception.report["windows"][1]["new_frames"], 0)
        self.assertFalse(raised.exception.report["capture_gate_pass"])

    def test_feedback_must_cover_both_boundaries_of_every_scored_window(self):
        ids, rows, source = measurement(30)
        for earliest, latest in [(63, 210), (1, 180), (63, 180)]:
            with self.subTest(earliest=earliest, latest=latest):
                clipped = "\n".join(
                    line
                    for line in source.splitlines()
                    if earliest <= int(line.split("id=")[1].split()[0]) <= latest
                )
                with self.assertRaisesRegex(
                    InvalidMeasurement,
                    "presentation feedback does not cover scored windows",
                ):
                    score(
                        ids,
                        rows,
                        clipped,
                        lambda ns: ns,
                        warmup=2,
                        seconds=4,
                        window=2,
                        fps=30,
                        refresh=30,
                    )

    def test_torn_frame_is_valid_and_both_ids_prevent_capture_misses(self):
        ids, rows, source = measurement(30, drop={71})
        ids[69] = Torn(70, 71)
        report = score(
            ids,
            rows,
            source,
            lambda ns: ns,
            warmup=2,
            seconds=4,
            window=2,
            fps=30,
            refresh=30,
        )
        self.assertEqual(report["windows"][0]["torn_frames"], 1)
        self.assertEqual(report["windows"][0]["new_frames"], 60)
        self.assertEqual(report["windows"][0]["capture_misses"], 0)
        self.assertTrue(report["capture_gate_pass"])
        self.assertEqual(report["torn_events"][0]["upper_id"], 70)
        self.assertEqual(report["torn_events"][0]["lower_id"], 71)

    def test_capture_drop_and_source_discard_are_separate(self):
        ids, rows, source = measurement(60, drop={151}, discard={181})
        report = score(
            ids,
            rows,
            source,
            lambda ns: ns,
            warmup=2,
            seconds=4,
            window=2,
            fps=60,
            refresh=60,
        )
        self.assertEqual(report["windows"][0]["capture_misses"], 1)
        self.assertEqual(report["windows"][0]["source_misses"], 1)
        self.assertEqual(report["windows"][0]["source_discarded_submissions"], 1)
        self.assertNotIn(181, [event["id"] for event in report["capture_events"]])

    def test_invalid_measurement_retains_torn_and_unreadable_diagnostics(self):
        ids, rows, source = measurement(30)
        ids[69] = Torn(70, 71)
        ids[70] = Unreadable("row 1 CRC failed")
        with self.assertRaises(InvalidMeasurement) as raised:
            score(
                ids,
                rows,
                source,
                lambda ns: ns,
                warmup=2,
                seconds=4,
                window=2,
                fps=30,
                refresh=30,
            )
        report = raised.exception.report
        self.assertEqual(report["torn_frames"], 1)
        self.assertEqual(report["windows"][0]["undecodable_frames"], 1)
        self.assertEqual(len(report["unreadable_events"]), 1)
        self.assertFalse(report["capture_gate_pass"])

    def test_unreadable_budget_accepts_95_but_never_counts_their_ids_as_present(self):
        ids, rows, source = measurement(30)
        missing = set(range(61, 156))
        for index in range(60, 155):
            ids[index] = Unreadable("row 0 CRC failed")
        arguments = dict(
            warmup=2, seconds=4, window=2, fps=30, refresh=30, max_unreadable_frames=200
        )
        report = score(ids, rows, source, lambda ns: ns + 5_000_000, **arguments)
        self.assertEqual(report["unreadable_frames"], 95)
        self.assertEqual(report["max_unreadable_frames"], 200)
        text = summary(
            {"label": "budget", "discovered": {"valid": True}, "source_counter": report}
        )
        self.assertIn("unreadable_frames=95 max_unreadable_frames=200", text)
        self.assertEqual(sum(w["new_frames"] for w in report["windows"]), 25)
        self.assertEqual(sum(w["capture_misses"] for w in report["windows"]), 95)
        self.assertEqual({event["id"] for event in report["capture_events"]}, missing)
        arguments["max_unreadable_frames"] = 95
        self.assertEqual(
            score(ids, rows, source, lambda ns: ns + 5_000_000, **arguments)[
                "unreadable_frames"
            ],
            95,
        )
        arguments["max_unreadable_frames"] = 94
        with self.assertRaises(InvalidMeasurement):
            score(ids, rows, source, lambda ns: ns + 5_000_000, **arguments)

    def test_bad_scored_id_or_count_invalidates(self):
        ids, rows, source = measurement(30)
        ids[70] = Unreadable("row 1 CRC failed")
        with self.assertRaisesRegex(ValueError, "unreadable scored"):
            score(
                ids,
                rows,
                source,
                lambda ns: ns,
                warmup=2,
                seconds=4,
                window=2,
                fps=30,
                refresh=30,
            )
        with self.assertRaisesRegex(ValueError, "differs from receive count"):
            score(
                ids[:-1],
                rows,
                source,
                lambda ns: ns,
                warmup=2,
                seconds=4,
                window=2,
                fps=30,
                refresh=30,
            )

    def test_expected_is_minimum_of_request_and_refresh(self):
        ids, rows, source = measurement(60)
        report = score(
            ids,
            rows,
            source,
            lambda ns: ns,
            warmup=2,
            seconds=4,
            window=2,
            fps=120,
            refresh=60,
        )
        self.assertEqual(report["windows"][0]["expected_new_frames"], 120)

    def test_bridge_interpolates_and_reports_uncertainty(self):
        a = dict(device_ns=100, host_before_ns=200, host_after_ns=220)
        b = dict(device_ns=200, host_before_ns=300, host_after_ns=320)
        convert, uncertainty = bridge(a, b)
        self.assertEqual(convert(150), 260)
        self.assertEqual(uncertainty, 0.00001)


class DiscoveryTests(unittest.TestCase):
    def test_real_sp_output_and_synthetic_high_refresh(self):
        text = (FIXTURES / "sp/outputs.json").read_text()
        parsed = output(text)
        self.assertEqual(parsed["refresh_hz"], 60)
        self.assertEqual(parsed["transform"], "normal")
        self.assertFalse(parsed["power"])
        rows = json.loads(text)
        rows[0]["current_mode"]["refresh"] = 120000
        self.assertEqual(output(json.dumps(rows))["refresh_hz"], 120)

    def test_real_proc_and_systemd_samples(self):
        text = (FIXTURES / "sp/compositor.txt").read_text()
        self.assertEqual(credentials(text), (1000, 1000))
        props = properties(
            (FIXTURES / "sp/sunshine-unit.txt").read_text().split("# /run/", 1)[0]
        )
        env = environment(props["Environment"])
        self.assertTrue(env["KORRI_CERTIFICATE_CONTROL_PATH"].endswith(".sock"))
        self.assertEqual(env["KORRI_CERTIFICATE_CONTROL_UID"], "976")

    def test_real_sp_failed_probe_does_not_use_old_working_encoder(self):
        text = (FIXTURES / "sp/journal.txt").read_text()
        self.assertEqual(encoder_probe(text), [])
        fresh = text[text.rfind("Sunshine version:") :]
        self.assertEqual(encoder_probe(fresh), [])
        self.assertEqual(
            encoder_probe("Found codec encoder: arbitrary-name"), ["arbitrary-name"]
        )

    def test_real_manual_restart_has_successful_probe(self):
        record = json.loads((FIXTURES / "sp/manual-restart.json").read_text())
        self.assertTrue(record["working_encoder_confirmed"])
        self.assertTrue(encoder_probe(record["fresh_journal"]))
        refusal = json.loads((FIXTURES / "sp/encoder-refusal.json").read_text())
        self.assertFalse(refusal["discovered"]["valid"])
        self.assertTrue(refusal["discovered"]["restore"]["pass"])

    def test_session_markers_and_encoder_are_not_asserted(self):
        text = "Creating encoder [arbitrary-name]\nScreencasting with some-backend"
        self.assertEqual(session_log(text)["encoder"], "arbitrary-name")
        self.assertEqual(session_log(text)["capture_backend"], "some-backend")
        reprobe = "Creating encoder [failed-probe]\n" + text
        self.assertEqual(session_log(reprobe)["encoder"], "arbitrary-name")
        self.assertEqual(session_log(text)["pacing"], "unmarked")
        from discovery import MARKERS

        self.assertEqual(
            session_log(text + "\n" + MARKERS["locked"])["pacing"], "locked"
        )
        self.assertEqual(
            session_log(text + "\n" + MARKERS["fallback"])["pacing"], "fallback"
        )

    def test_tcp_inode_parser(self):
        header = (
            "sl local_address rem_address st tx_queue rx tr tm retr uid timeout inode\n"
        )
        line = "0: 00000000:ABCD 00000000:0000 0A 0 0 0 0 0 12345\n"
        self.assertEqual(tcp_ports(header + line, {"12345"}), [0xABCD])
        self.assertEqual(tcp_ports(header + line, {"54321"}), [])


class CommandTests(unittest.TestCase):
    def test_unreadable_limit_defaults_to_zero_and_rejects_negative_values(self):
        self.assertEqual(
            parser().parse_args(["run", "--device", "device"]).max_unreadable_frames, 0
        )
        self.assertEqual(
            parser()
            .parse_args(["run", "--device", "device", "--max-unreadable-frames", "200"])
            .max_unreadable_frames,
            200,
        )
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            parser().parse_args(
                ["run", "--device", "device", "--max-unreadable-frames", "-1"]
            )

    def test_ssh_options_repeat_and_keep_native_option_values_intact(self):
        settings = [
            'UserKnownHostsFile="/tmp/cadence known hosts"',
            "GlobalKnownHostsFile=/dev/null",
        ]
        args = parser().parse_args(
            [
                "run",
                "--device",
                "example.test:2345",
                "--ssh-option",
                settings[0],
                "--ssh-option",
                settings[1],
            ]
        )
        self.assertEqual(args.ssh_option, settings)
        target, options = ssh_target(args.device, args.ssh_port, args.ssh_option)
        self.assertEqual(target, "example.test")
        self.assertEqual(
            options[-6:], ["-o", settings[0], "-o", settings[1], "-p", "2345"]
        )
        configured = subprocess.run(
            ["ssh", "-G", "-F", "/dev/null", *options, target],
            check=True,
            text=True,
            capture_output=True,
        ).stdout
        self.assertIn("userknownhostsfile /tmp/cadence known hosts", configured)

    def test_device_options_reach_nix_copy_without_a_device_connection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "nix"
            binary.write_text(
                f"#!{sys.executable}\n"
                + Path(__file__).with_name("nix-transport-peer.py").read_text()
            )
            binary.chmod(0o700)
            log = root / "commands.jsonl"
            target, options = ssh_target(
                "operator@example.test:2345",
                None,
                ['UserKnownHostsFile="/tmp/cadence known hosts"'],
            )
            previous = dict(os.environ)
            try:
                os.environ.update(
                    PATH=str(root) + os.pathsep + os.environ["PATH"],
                    HOME=str(root),
                    CADENCE_NIXPKGS="/nix/store/transport-nixpkgs",
                    CADENCE_TRANSPORT_LOG=str(log),
                )
                with contextlib.redirect_stdout(io.StringIO()):
                    _, _, imported = tools("aarch64", target, options)
            finally:
                os.environ.clear()
                os.environ.update(previous)
            calls = [json.loads(line) for line in log.read_text().splitlines()]
            copy = next(call for call in calls if call["arguments"][0] == "copy")
            self.assertEqual(
                copy["arguments"][1:3], ["--to", "ssh://operator@example.test"]
            )
            self.assertEqual(shlex.split(copy["ssh_options"]), options)
            self.assertEqual(imported, copy["arguments"][3:])
            self.assertEqual(
                [call["arguments"][0] for call in calls], ["build", "eval", "copy"]
            )
            self.assertIn("--no-link", calls[0]["arguments"])
            self.assertNotIn(
                "--add-root",
                [argument for call in calls for argument in call["arguments"]],
            )

    def test_bitrate_default_is_explicit_and_can_be_overridden(self):
        args = parser().parse_args(["run", "--device", "device"])
        self.assertEqual(args.bitrate, 10000)
        args = parser().parse_args(["run", "--device", "device", "--bitrate", "2000"])
        self.assertEqual(args.bitrate, 2000)

    def test_torn_limit_is_explicit_and_counts_each_new_window(self):
        args = parser().parse_args(
            ["run", "--device", "device", "--max-torn-frames", "0"]
        )
        self.assertEqual(args.max_torn_frames, 0)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)

            def record(torn):
                return {
                    "source_counter": {
                        "scored_seconds": 10,
                        "windows": [
                            dict(
                                new_frames=600,
                                capture_misses=0,
                                source_misses=0,
                                torn_frames=torn,
                            )
                        ],
                    }
                }

            (root / "old").write_text(json.dumps(record(1)))
            (root / "new").write_text(json.dumps(record(3)))
            args = parser().parse_args(
                ["compare", str(root / "old"), str(root / "new")]
            )
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(compare(args), 0)
                args.max_torn_frames = 2
                self.assertEqual(compare(args), 1)
                args.max_torn_frames = 3
                self.assertEqual(compare(args), 0)

    def test_port_suffix_and_separate_option(self):
        target, options = ssh_target("operator@example.test:2345", None)
        self.assertEqual(target, "operator@example.test")
        self.assertEqual(options[-2:], ["-p", "2345"])
        self.assertEqual(ssh_target("operator@example.test", 2345), (target, options))
        with self.assertRaises(ValueError):
            ssh_target("-oProxyCommand=bad", None)
        with self.assertRaises(ValueError):
            ssh_target("host:2345", 2345)

    def test_latency_limit_fails_closed_when_either_record_has_no_latency(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for left, right in [(None, 5), (5, None), (None, None)]:
                with self.subTest(left=left, right=right):
                    for name, value in [("old", left), ("new", right)]:
                        (root / name).write_text(
                            json.dumps(
                                {
                                    "source_counter": {
                                        "windows": [
                                            dict(
                                                seconds=10,
                                                new_frames=0,
                                                capture_misses=0,
                                                source_misses=0,
                                                latency_ms={"p50": value},
                                            )
                                        ]
                                    }
                                }
                            )
                        )
                    args = parser().parse_args(
                        [
                            "compare",
                            str(root / "old"),
                            str(root / "new"),
                            "--max-latency-increase-ms",
                            "1",
                        ]
                    )
                    output = io.StringIO()
                    with contextlib.redirect_stdout(output):
                        self.assertEqual(compare(args), 1)
                    self.assertIn("latency unavailable", output.getvalue())

    def test_compare_only_fails_with_explicit_limit(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            old = {
                "source_counter": {
                    "scored_seconds": 10,
                    "windows": [
                        dict(new_frames=600, capture_misses=0, source_misses=0)
                    ],
                }
            }
            new = {
                "source_counter": {
                    "scored_seconds": 10,
                    "windows": [
                        dict(new_frames=500, capture_misses=100, source_misses=0)
                    ],
                }
            }
            (root / "old").write_text(json.dumps(old))
            (root / "new").write_text(json.dumps(new))
            args = parser().parse_args(
                ["compare", str(root / "old"), str(root / "new")]
            )
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(compare(args), 0)
                args.max_fps_drop = 1
                self.assertEqual(compare(args), 1)


if __name__ == "__main__":
    unittest.main()
