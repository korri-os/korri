#!/usr/bin/env python3
"""Host-only safety/verdict checks. No systemd, SSH, input devices or builds."""
import base64
import importlib.util
import json
import subprocess
import stat
from types import SimpleNamespace
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("native_iteration", Path(__file__).with_name("run.py"))
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)
PARSER = Path(__file__).resolve().parents[4] / "clients/linux/diagnostics/native-verdict.py"


class Safety(unittest.TestCase):
    def test_only_exact_terminal_rpc_replies_are_idle(self):
        for code in ("NoActiveSession", "SessionCompleted"):
            value = {"_tag": "app.session.status", "outcome": {"_tag": "Err", "payload": {"code": code}}}
            self.assertTrue(runner.idle_reply(value))
            value["outcome"]["_tag"] = "Ok"
            self.assertFalse(runner.idle_reply(value))
        for value in (None, {}, {"outcome": None}, {"_tag": "other"},
                      {"_tag": "app.session.status", "outcome": {"_tag": "Err", "payload": {"code": "Unavailable"}}}):
            self.assertFalse(runner.idle_reply(value))

    def test_checksum_paths_cannot_escape_stage(self):
        for name in ("../secret", "/etc/passwd", "bin/../../secret", "baseline/state.json"):
            with self.subTest(name=name), self.assertRaises(RuntimeError):
                list(runner.payload_records("0" * 64 + "  " + name + "\n"))

    def test_metadata_namespace_denial_is_included_with_exact_grants(self):
        text = 'action.id.indexOf("org.shadowblip.") == 0; ["org.shadowblip.Input.Target.DevicePaths"]'
        self.assertEqual(runner.metadata_actions(text),
                         {"org.shadowblip.", "org.shadowblip.Input.Target.DevicePaths"})

    def test_duplicate_manifest_entry_is_rejected(self):
        line = "0" * 64 + "  bin/korrid\n"
        with self.assertRaises(RuntimeError):
            list(runner.payload_records(line + line))

    def test_elf_rejects_host_architecture_and_scripts(self):
        for data in (b"#!/usr/bin/env bash\n", b"\x7fELF\x02\x01" + bytes(100)):
            with self.assertRaises(RuntimeError):
                runner.elf_interpreter(data)

    def test_runtime_paths_reject_unit_expansion_and_newlines(self):
        for path in ("relative", "/run/x\nExecStart=evil", "/run/%n", "/run/../etc/x", "/run/a:b"):
            with self.assertRaises(RuntimeError):
                runner.guarded_path(path)

    def test_rollback_never_unlinks_replaced_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "override"
            path.write_text("owned")
            baseline = runner.fingerprint(path)
            path.write_text("another operator's change")
            with self.assertRaises(RuntimeError):
                runner.remove_owned(path, baseline)
            self.assertTrue(path.exists())

    def test_symlink_identity_is_not_target_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "target"
            target.write_text("x")
            link = Path(tmp) / "link"
            link.symlink_to(target)
            baseline = runner.fingerprint(link)
            self.assertEqual(baseline["link"], str(target))
            link.unlink()
            link.write_text("x")
            with self.assertRaises(RuntimeError):
                runner.remove_owned(link, baseline)

    def test_owned_file_cleanup_is_idempotent(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "override"
            path.write_text("owned")
            baseline = runner.fingerprint(path)
            runner.remove_owned(path, baseline)
            runner.remove_owned(path, baseline)
            self.assertFalse(path.exists())

    def test_cutover_order_finishes_old_receiver_before_provider(self):
        calls = []
        with patch.object(runner, "systemctl", side_effect=lambda *args: calls.append(args)), \
             patch.object(runner, "show", return_value="inactive"):
            runner.stop_order({"old_receiver": "signed-receiver.service"})
        self.assertEqual(calls, [("stop", unit) for unit in (
            runner.INPUTD, runner.KIOSK, runner.SUNSHINE, runner.CONTROL,
            runner.BRAIN, "signed-receiver.service", runner.PROVIDER)])

    def test_private_socket_snapshot_never_queries_main_pid(self):
        def show(unit, prop):
            self.assertEqual(unit, runner.CONTROL)
            self.assertNotEqual(prop, "MainPID")
            return "observed"
        with patch.object(runner, "show", side_effect=show), \
             patch.object(runner, "executable", side_effect=AssertionError("socket has no executable")):
            info = runner.unit_snapshot(runner.CONTROL)
        self.assertIsNone(info["executable"])
        self.assertEqual(info["SocketMode"], "observed")
        self.assertEqual(info["Triggers"], "observed")

    def test_socket_metadata_preserves_nonroot_group_and_mode(self):
        with patch.object(Path, "stat", return_value=SimpleNamespace(
                st_mode=stat.S_IFSOCK | 0o660, st_uid=0, st_gid=976)):
            self.assertEqual(runner.socket_metadata("/unused"), {"uid": 0, "gid": 976, "mode": 0o660})

    def test_status_poll_cannot_resurrect_old_brain_during_cutover(self):
        active = {unit: True for unit in (runner.INPUTD, runner.KIOSK, runner.SUNSHINE,
                  runner.CONTROL, runner.BRAIN, "signed.service", runner.PROVIDER)}
        def stop(verb, unit):
            self.assertEqual(verb, "stop")
            active[unit] = False
            # Reproduce the observed old-inputd poll / socket activation race.
            if unit == runner.BRAIN and active[runner.INPUTD] and active[runner.CONTROL]:
                active[runner.BRAIN] = True
        with patch.object(runner, "systemctl", side_effect=stop), \
             patch.object(runner, "show", side_effect=lambda unit, prop: "active" if active[unit] else "inactive"):
            runner.stop_order({"old_receiver": "signed.service"})
        self.assertFalse(any(active.values()))

    def test_existing_brain_is_not_accepted_as_a_fresh_start(self):
        with patch.object(runner, "show", return_value="active"), patch.object(runner, "start") as start:
            with self.assertRaisesRegex(RuntimeError, "already running"):
                runner.start_brain({}, "/expected/korrid")
        start.assert_not_called()

    def test_control_socket_starts_first_and_real_listener_fd_is_required(self):
        metadata = {"uid": 0, "gid": 977, "mode": 0o660}
        def show(unit, prop):
            return "123" if prop == "MainPID" else "inactive"
        with patch.object(runner, "show", side_effect=show), \
             patch.object(runner, "start") as starts, \
             patch.object(runner, "wait_idle_ready"), \
             patch.object(runner, "socket_metadata", return_value=metadata), \
             patch.object(runner, "executable", return_value="/expected/korrid"), \
             patch.object(Path, "read_text", return_value="header\n0000: 2 0 10000 0001 01 42 /run/korrid-control/control.sock\n"), \
             patch.object(Path, "iterdir", return_value=iter([Path("/proc/123/fd/3")])), \
             patch.object(runner.os, "readlink", return_value="socket:[42]"):
            runner.start_brain({"control_socket": metadata}, "/expected/korrid")
        self.assertEqual([call.args for call in starts.call_args_list], [(runner.CONTROL,), (runner.BRAIN,)])

    def test_parent_wants_partof_cannot_start_kiosk_through_closed_gate(self):
        # These are the actual declarations responsible for the device failure.
        module = (PARSER.parents[3] / "clients/portal/nix/nixos-module.nix").read_text()
        self.assertIn('wantedBy = [ "multi-user.target" ] ++ kioskParents;', module)
        self.assertIn('partOf = kioskParents;', module)
        parents = module.split("kioskParents = [", 1)[1].split("];", 1)[0]
        self.assertIn('"korrid.service"', parents)
        self.assertIn('"nginx.service"', parents)
        with tempfile.TemporaryDirectory() as tmp:
            stage = Path(tmp)
            gate = stage / "gate.conf"
            state = {"owned": {}, "removed": []}
            active = {runner.KIOSK: False}
            def install(stage_arg, state_arg, destination, source):
                gate.write_bytes(source.read_bytes())
                state_arg["owned"][str(gate)] = runner.fingerprint(gate)
            def systemctl(verb, *units):
                if verb == "start":
                    # Model actual parent Wants and kiosk's native condition.
                    for unit in units:
                        if unit in (runner.BRAIN, "nginx.service", runner.KIOSK):
                            active[runner.KIOSK] = (stage / "control/kiosk-allow").exists()
                if verb == "stop" and runner.BRAIN in units:
                    active[runner.KIOSK] = False  # Actual PartOf propagation.
            def show(unit, prop):
                if prop == "DropInPaths":
                    return str(gate)
                return "active" if active.get(unit, False) else "inactive"
            with patch.object(runner, "KIOSK_GATE", str(gate)), \
                 patch.object(runner, "owned_install", side_effect=install), \
                 patch.object(runner, "save_state"), \
                 patch.object(runner, "systemctl", side_effect=systemctl), \
                 patch.object(runner, "show", side_effect=show):
                runner.install_kiosk_gate(stage, state)
                self.assertEqual(gate.read_text(), f"[Unit]\nConditionPathExists={stage}/control/kiosk-allow\n")
                systemctl("start", runner.BRAIN)
                systemctl("start", "nginx.service")
                self.assertFalse(active[runner.KIOSK])
                runner.open_candidate_kiosk(stage)
                self.assertTrue(active[runner.KIOSK])
                # Keep root ownership checks in production; fixture is host-owned.
                (stage / "control/kiosk-allow").unlink()
                systemctl("stop", runner.BRAIN)
                systemctl("start", runner.BRAIN)
                self.assertFalse(active[runner.KIOSK])
                self.assertTrue(gate.exists())

    def test_view_waits_for_late_sunshine_pad_and_stable_producer_pids(self):
        now = [0.0]
        samples = iter(["old-view", "old-view", "view-with-absolute-mouse"])
        def view():
            return next(samples, "view-with-absolute-mouse")
        with patch.object(runner.time, "monotonic", side_effect=lambda: now[0]), \
             patch.object(runner.time, "sleep", side_effect=lambda duration: now.__setitem__(0, now[0] + duration)), \
             patch.object(runner, "process_identities", return_value={runner.SUNSHINE: ["456", "/signed/sunshine"]}), \
             patch.object(runner, "view_text", side_effect=view):
            topology, _ = runner.settled_view({runner.SUNSHINE: "/signed/sunshine"})
        self.assertEqual(topology, "view-with-absolute-mouse")
        self.assertGreaterEqual(now[0], 3)

    def test_changing_topology_hits_bounded_deadline(self):
        now = [0.0]
        with patch.object(runner.time, "monotonic", side_effect=lambda: now[0]), \
             patch.object(runner.time, "sleep", side_effect=lambda duration: now.__setitem__(0, now[0] + duration)), \
             patch.object(runner, "process_identities", return_value={}), \
             patch.object(runner, "view_text", side_effect=lambda: str(now[0])):
            with self.assertRaisesRegex(RuntimeError, "did not settle"):
                runner.settled_view({}, seconds=3)
        self.assertEqual(now[0], 3)

    def test_readiness_diagnostics_exclude_raw_values_and_socket_pid(self):
        calls = []
        def command(*argv, **kwargs):
            calls.append(argv)
            return subprocess.CompletedProcess([], 0,
                b"ActiveState=active\nSubState=running\nMainPID=123\nResult=success\nEnvironment=SECRET\nStatusText=SECRET\n", b"SECRET")
        with tempfile.TemporaryDirectory() as tmp:
            stage = Path(tmp)
            with patch.object(runner, "command", side_effect=command), \
                 patch.object(runner.os, "readlink", return_value="/nix/store/example/bin/korrid"):
                runner.readiness_diagnostics(stage, "trial")
            record = (stage / "evidence/readiness-trial.json").read_text()
            self.assertNotIn("SECRET", record)
            self.assertNotIn("MainPID", json.dumps(json.loads(record)[runner.CONTROL]))
        socket_calls = [call for call in calls if runner.CONTROL in call]
        self.assertTrue(socket_calls)
        self.assertNotIn("--property=MainPID", socket_calls[0])

    def test_gate_and_rollback_topology_order(self):
        source = Path(__file__).with_name("run.py").read_text()
        trial = source.split("def trial(stage, args):", 1)[1].split("def view_text", 1)[0]
        self.assertLess(trial.index("install_kiosk_gate(stage, state)"), trial.index("stop_order(state)"))
        restore = source.split("def restore(stage):", 1)[1].split("def verify(stage):", 1)[0]
        self.assertIn('path == KIOSK_GATE', restore)
        self.assertLess(restore.index("start(SUNSHINE)"), restore.index("current_topology, identities = settled_view"))
        self.assertLess(restore.index("os.replace(current_view, VIEW)"), restore.index("remove_owned(KIOSK_GATE"))
        self.assertLess(restore.index("remove_owned(KIOSK_GATE"), restore.index("start(KIOSK)"))
        self.assertLess(restore.index("start_brain(state, state[\"units\"][BRAIN][\"executable\"])"), restore.index('systemctl("restart", "nginx.service")'))

    def test_coordinator_readiness_retries_only_transport_failure(self):
        idle = {"_tag": "app.session.status", "outcome": {"_tag": "Err", "payload": {"code": "NoActiveSession"}}}
        replies = [subprocess.CompletedProcess([], 7, b"", b"unavailable"),
                   subprocess.CompletedProcess([], 0, json.dumps(idle).encode(), b"")]
        with patch.object(runner, "show", side_effect=lambda unit, prop: "123" if prop == "MainPID" else "active"), \
             patch.object(runner, "executable", return_value="/expected/korrid"), \
             patch.object(runner, "command", side_effect=replies) as calls, \
             patch.object(runner.time, "sleep"):
            runner.wait_idle_ready("/expected/korrid")
        self.assertEqual(calls.call_count, 2)

    def test_real_nonterminal_reply_never_becomes_idle(self):
        active = {"_tag": "app.session.status", "outcome": {"_tag": "Ok", "payload": {"active": {"phase": "running"}}}}
        with patch.object(runner, "show", side_effect=lambda unit, prop: "123" if prop == "MainPID" else "active"), \
             patch.object(runner, "executable", return_value="/expected/korrid"), \
             patch.object(runner, "command", return_value=subprocess.CompletedProcess([], 0, json.dumps(active).encode(), b"")) as calls:
            with self.assertRaisesRegex(RuntimeError, "not proven idle"):
                runner.wait_idle_ready("/expected/korrid")
        self.assertEqual(calls.call_count, 1)

    def test_core_rule_install_follows_old_cleanup(self):
        # Static regression only, not a claim about actual systemd ordering.
        source = Path(__file__).with_name("run.py").read_text()
        cutover = source.split("def trial(stage, args):", 1)[1].split("def view_text", 1)[0]
        self.assertLess(cutover.index("stop_order(state)"), cutover.index('rules = "/run/udev/'))
        self.assertNotIn("nix build", source)
        self.assertNotIn("StrictHostKeyChecking=no", source)


class HostVerdict(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.evidence = Path(self.temp.name)
        for name, data in (("restore.log", "RESTORED: baseline\n"),
                           ("verification.log", "ROLLBACK VERIFIED: independently\n"),
                           ("capture", "CAPTURE COMPLETE: physical only\n")):
            (self.evidence / name).write_text(data)
        (self.evidence / "screen.b64").write_bytes(base64.b64encode(b"\xff\xd8\xfffixture\xff\xd9"))
        self.samples = []
        for i in range(13):
            state = {"hasFocus": True, "visible": True, "activeIsControl": True,
                     "countersPresent": True, "nativeStarts": 1, "gamepadStarts": 0,
                     "gamepadReads": 0, "nativeSamples": i, "nativeInitializations": 1,
                     "focusNode": 1 if i < 6 else 2}
            self.samples.append((1_000_000 + 250_000 * i, state))
        self.write_observer()
        (self.evidence / "replay.log").write_text(
            "REPLAY_BEGIN_MONOTONIC_US=1500000\nREPLAY_END_MONOTONIC_US=3500000\n"
            "REPLAY WRITES COMPLETE: four Right pulses; kernel/browser delivery NOT asserted\n")

    def write_observer(self):
        (self.evidence / "observer.log").write_text("".join(
            f"[DEBUG-native-input] monotonic_us={stamp} state={json.dumps(state)}\n"
            for stamp, state in self.samples))

    def verdict(self):
        return runner.host_verdict(self.evidence, PARSER)

    def test_native_pass_is_explicitly_not_remote_acceptance(self):
        self.assertIn("NOT remote Sunshine acceptance", self.verdict())

    def test_capture_or_restore_claim_alone_is_insufficient(self):
        (self.evidence / "verification.log").write_text("not checked")
        with self.assertRaises(RuntimeError):
            self.verdict()

    def test_old_gamepad_probe_is_not_native_evidence(self):
        (self.evidence / "observer.log").write_text('[DEBUG-rpmini-input] queried_gamepads=true state={"hasFocus":true}\n')
        with self.assertRaises(RuntimeError):
            self.verdict()

    def test_native_counter_without_dom_focus_transition_fails(self):
        for _, state in self.samples:
            state["focusNode"] = 1
        self.write_observer()
        with self.assertRaises(RuntimeError):
            self.verdict()

    def test_browser_gamepad_read_fails(self):
        self.samples[5][1]["gamepadReads"] = 1
        self.write_observer()
        with self.assertRaises(RuntimeError):
            self.verdict()

    def test_missing_real_screenshot_fails(self):
        (self.evidence / "screen.b64").write_text("")
        with self.assertRaises(RuntimeError):
            self.verdict()


if __name__ == "__main__":
    unittest.main()
