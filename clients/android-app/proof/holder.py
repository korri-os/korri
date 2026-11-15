#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Slice D9 of #434: the app binds to the test owner and browses a holder (ADR 0063).

It runs on aka, which carries the emulator and the private sender (owner rule
of 2026-11-15: no heavy work on zao), as root, with no person. The pieces are
existing ones:

- the holder: korrid's federation fixture (services/korrid/examples/
  federation_fixture.rs), a real host whose owner is the fixture's person
  signer (secret 3), with its NIP-01 relay from the Artemis app's federation
  acceptance (holder.ts runs both);
- the private sender: stream-cadence's sender fixture with korrid's
  application, `Korri Stream`, showing the frame-ID source. The holder's one
  game streams from it: its endpoint record carries the sender's address;
- the test device's owner binding: the fixture's own producer
  (`federation_fixture member`), seeded into the app's private state through
  `run-as`, as the route brief's D9 says. No signer route is involved;
- the app: the fixture APK, the product's library with one loopback relay,
  which `adb reverse` maps to the holder's relay.

The steps:

1. Build the fixture APK, the federation fixture and the sender's parts.
   Read the APK: no dex, `hasCode="false"`, the one loopback relay, and the
   TLS verifier's classes in the library.
2. Start the sender fixture with `Korri Stream` and its picture source, and
   read its `serverinfo` at the address the holder publishes.
3. Start the holder and its relay. Make a member's state with the holder's
   owner.
4. Start one headless Android 12 emulator (2 cores, its own adb server, its
   data wiped), install the APK signed with the debug key, seed the member's
   identity, map the relay and the holder's port into it, and start the app.
5. Wait until Velvet's library lists the holder's game, and take a
   screenshot. Read what the app's korrid published to the relay (its owner
   statement, signed by the test owner) and what the holder sent it (its
   endpoint record), and the peer that the app's korrid remembers.
6. Stop the emulator, its adb server, the holder and the sender.

Starting the stream is D10. The record goes to --out; no private key enters
it. The exit status is 0 only when every check passed.
"""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import run as d8  # noqa: E402  (slice D8's proof: the emulator, logcat, signing)


def module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


ROOT = Path(os.environ["KORRI_ROOT"])
# The viewer proof's sender agent client (stream-cadence's agent protocol).
viewer_proof = module("viewer_proof", ROOT / "clients/viewer/android/proof/run.py")

PACKAGE = d8.PACKAGE
IDENTITY = "no_backup/korrid-state/identity"
# The public key of secret 3, the fixture's person signer.
OWNER = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9"
LABEL = "federation-acceptance"
APPLICATION = "Korri Stream"
VERIFIER_CLASS = b"Lorg/rustls/platformverifier/CertificateVerifier;"
VERIFIER_LOADED = "korrid: Android TLS verifier classes loaded"
# The holder's Moonlight address with no sender (federation_fixture.rs).
NO_VIDEO = "127.0.0.1:9"
LIBRARY_LISTS_A_GAME = re.compile(r"korri-app: shell rendered surface=velvet page=home count=([1-9]\d*) selected=(-?\d+)")
log = d8.log


def stream_host() -> str:
    """This host's address on its default route, as the emulator reaches it."""
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as probe:
        probe.connect(("192.0.2.1", 9))  # no packet leaves: a UDP connect only picks the route
        return probe.getsockname()[0]


def serverinfo(address: str) -> str:
    """Sunshine's unpaired serverinfo answer at the holder's Moonlight address."""
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(f"http://{address}/serverinfo?uniqueid=0123456789ABCDEF", timeout=10) as answer:
        return answer.read(1 << 16).decode(errors="replace")


class Holder:
    """holder.ts: the federation fixture's host and its relay.

    `user` is the uid and gid that the holder runs as (D10: an unprivileged
    user, so that korrid's launch helpers accept its runtime user); the run
    directory becomes theirs. `environment` adds to the caller's environment
    what holder.ts passes on (the certificate control of the sender).
    """

    def __init__(self, run: Path, executable: str, moonlight: str,
                 user: "tuple[int, int] | None" = None, environment: "dict[str, str] | None" = None):
        self.run = run
        drop = None
        environment = dict(environment or {})
        if user is not None:
            uid, gid = user
            os.chown(run, uid, gid)
            # bun's own caches go to the user's run directory, not the caller's home.
            environment.update(HOME=str(run), XDG_CACHE_HOME=str(run / "cache"))

            def drop() -> None:
                os.setgroups([gid])
                os.setgid(gid)
                os.setuid(uid)
        self.process = subprocess.Popen(
            ["bun", str(HERE / "holder.ts"), str(run), str(ROOT), executable, moonlight],
            stdout=(run / "holder-ts.log").open("w"), stderr=subprocess.STDOUT, start_new_session=True,
            preexec_fn=drop, env=dict(os.environ, **environment),
        )
        until = time.monotonic() + 60
        while not (run / "ready").exists():
            if self.process.poll() is not None or time.monotonic() > until:
                raise RuntimeError("the holder did not start; read holder-ts.log and holder.log")
            time.sleep(0.2)
        port, key, relay = (run / "ready").read_text().split()
        self.port, self.key, self.relay = int(port), key, int(relay)

    def stop(self) -> dict:
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGTERM)
            try:
                self.process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
                self.process.wait(timeout=10)
        events = self.run / "events.json"
        return {
            "exit_code": self.process.returncode,
            "events": json.loads(events.read_text()) if events.is_file() else None,
        }


def changed(before: Path, after: Path) -> float:
    """The share of a coarse grid of pixels that differ between two screenshots."""
    from PIL import Image
    with Image.open(before) as a, Image.open(after) as b:
        small = [image.convert("L").resize((108, 240)) for image in (a, b)]
    pairs = list(zip(small[0].get_flattened_data(), small[1].get_flattened_data()))
    return round(sum(abs(x - y) > 24 for x, y in pairs) / len(pairs), 3)


def tag(event: dict, name: str) -> str:
    return next((t[1] for t in event["tags"] if t and t[0] == name and len(t) > 1), "")


def run_as(emulator: "d8.Emulator", *command: str) -> subprocess.CompletedProcess:
    return emulator.shell(shlex.join(["run-as", PACKAGE, *command]), check=False)


def seed(emulator: "d8.Emulator", member: Path) -> None:
    """The member's identity into the app's private state, written by the app's own user."""
    # The package manager answers run-as only once it lists the new package.
    until = time.monotonic() + 60
    while run_as(emulator, "true").returncode != 0:
        if time.monotonic() > until:
            raise RuntimeError("run-as does not know the installed package")
        time.sleep(1)
    made = run_as(emulator, "sh", "-c", f"umask 077 && mkdir -p {IDENTITY}")
    if made.returncode != 0:
        raise RuntimeError(f"run-as could not make {IDENTITY}: {made.stderr.strip()!r}")
    for name in ("device.key", "owner.event.json"):
        local = member / "identity" / name
        with local.open("rb") as data:
            write = shlex.join(["run-as", PACKAGE, "sh", "-c", f"umask 077 && cat > {IDENTITY}/{name}"])
            subprocess.run([f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "exec-in", write],
                           stdin=data, check=True, env=emulator.env, timeout=60)
        # adb exec-in returns neither the device command's status nor waits
        # for its end: read the size back until it is all there.
        until = time.monotonic() + 15
        while True:
            read = run_as(emulator, "wc", "-c", f"{IDENTITY}/{name}")
            written = read.stdout.split()
            if written and int(written[0]) == local.stat().st_size:
                break
            if time.monotonic() > until:
                raise RuntimeError(f"the app's storage did not take {name}: {local.stat().st_size} bytes sent, "
                                   f"wc says {read.stdout.strip()!r} {read.stderr.strip()!r}")
            time.sleep(0.5)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--port", type=int, default=5640, help="the emulator's console port")
    parser.add_argument("--adb-port", type=int, default=5141, help="this proof's own adb server port")
    parser.add_argument("--stream-host", help="the sender's address as the emulator reaches it; default: this host's")
    parser.add_argument("--library-seconds", type=int, default=300, help="how long Velvet may take to list the game")
    parser.add_argument("--no-sender", action="store_true",
                        help="start no sender fixture (for a hot aka): the holder publishes 127.0.0.1:9, which serves no video")
    parser.add_argument("--out", type=Path, help="the record's directory; default: a new one under /tmp")
    args = parser.parse_args()
    sdk = os.environ["KORRI_APP_EMULATOR_SDK"]
    image = os.environ["KORRI_APP_EMULATOR_IMAGE"]
    build_tools = Path(sdk, "build-tools", os.environ["KORRI_APP_BUILD_TOOLS"])
    # Its own adb server: other agents' emulators keep theirs.
    os.environ["ANDROID_ADB_SERVER_PORT"] = str(args.adb_port)
    host = args.stream_host or stream_host()
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-app-d9-"))
    out.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="korri-app-d9-run-"))
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "korri/android-app"
    log(f"record: {out}")
    result: dict = {"stream_host": host, "checks": {}, "steps": {}}
    checks, steps = result["checks"], result["steps"]

    apk = d8.nix_build(ROOT, "korri-android-app-fixture-apk") + "/korri-android-app-unsigned.apk"
    fixture = d8.nix_build(ROOT, "korrid.federationFixture") + "/bin/federation_fixture"
    result["store"] = {"apk": apk, "federation_fixture": fixture}
    signed = d8.sign(build_tools, apk, out / "korri-android-app-fixture-debug.apk")
    badging = d8.run([str(build_tools / "aapt2"), "dump", "badging", str(signed)]).stdout
    (out / "apk-badging.txt").write_text(badging)
    entries = d8.run(["unzip", "-l", str(signed)]).stdout
    (out / "apk-contents.txt").write_text(entries)
    manifest = d8.run([str(build_tools / "aapt2"), "dump", "xmltree", str(signed), "--file", "AndroidManifest.xml"]).stdout
    relays = json.loads(d8.run(["unzip", "-p", str(signed), "assets/relays.json"]).stdout)
    result["apk_relays"] = relays
    checks["apk_has_no_dex"] = ".dex" not in entries
    checks["apk_has_code_false"] = re.search(r"hasCode.*=false", manifest) is not None
    checks["apk_has_one_loopback_relay"] = len(relays) == 1 and re.fullmatch(r"ws://127\.0\.0\.1:\d+", relays[0]) is not None
    library = subprocess.run(["unzip", "-p", str(signed), f"lib/x86_64/{d8.LIBRARY}"], check=True, capture_output=True).stdout
    checks["library_carries_the_tls_verifiers_classes"] = VERIFIER_CLASS in library and b"Lkotlin/enums/EnumEntriesKt;" in library
    del library
    device_relay = int(relays[0].rsplit(":", 1)[1])

    sender = None
    if args.no_sender:
        result["sender"] = "not started (--no-sender): the holder's game has no live sender in this run"
    else:
        command, parts = viewer_proof.sender_tools(ROOT, "local")
        sender = viewer_proof.Remote(command, out / "sender-agent.log")
    emulator = d8.Emulator(sdk, state, args.port, out)
    holder: Holder | None = None
    logcat = None
    member = scratch / "member"
    try:
        moonlight = NO_VIDEO
        if sender is not None:
            log("starting the sender fixture with korrid's application")
            prepared = sender.request("prepare", sway=f"{parts['sway']}/bin/sway",
                                      sunshine=f"{parts['sunshine']}/bin/sunshine",
                                      application=APPLICATION, width=1280, height=720, fps=60)
            facts = prepared["facts"]
            (out / "sender-facts.json").write_text(json.dumps(facts, indent=2))
            sender.request("start", motion=f"{parts['motion']}/bin/stream-cadence-motion")
            moonlight = f"{host}:{facts['sunshine_port']}"
            checks["sender_offers_korri_stream"] = facts["sender_fixture"]["application"] == APPLICATION
            info = serverinfo(moonlight)
            (out / "sender-serverinfo.xml").write_text(info)
            checks["sender_answers_at_the_holders_moonlight_address"] = "<root" in info and 'status_code="200"' in info
        result["moonlight_address"] = moonlight

        log("starting the holder and its relay")
        holder = Holder(scratch, fixture, moonlight)
        result["holder"] = {"port": holder.port, "device": holder.key, "relay_port": holder.relay}
        device = d8.run([fixture, "member", str(member)]).stdout.strip()
        result["member_device"] = device
        checks["member_is_a_new_device"] = re.fullmatch(r"[0-9a-f]{64}", device) is not None and device != holder.key

        emulator.create(image)
        emulator.start()
        result["emulator"] = {
            "release": emulator.shell("getprop ro.build.version.release").stdout.strip(),
            "sdk": emulator.shell("getprop ro.build.version.sdk").stdout.strip(),
            "abi": emulator.shell("getprop ro.product.cpu.abi").stdout.strip(),
        }
        emulator.adb("uninstall", PACKAGE, check=False, timeout=120)
        installed = emulator.adb("install", "--no-incremental", str(signed), check=False, timeout=600)
        (out / "adb-install.txt").write_text(installed.stdout + installed.stderr)
        if installed.returncode != 0:
            raise RuntimeError("adb install failed; read adb-install.txt")
        seed(emulator, member)
        # The member's private key stays in the app's storage only.
        shutil.rmtree(member)
        listed = run_as(emulator, "ls", "-l", IDENTITY).stdout
        (out / "seeded-identity.txt").write_text(listed)
        checks["identity_seeded_as_the_apps_own_files"] = listed.count("-rw-------") == 2
        emulator.adb("reverse", f"tcp:{device_relay}", f"tcp:{holder.relay}")
        emulator.adb("reverse", f"tcp:{holder.port}", f"tcp:{holder.port}")

        logcat = d8.Logcat(emulator, out / "logcat-all.txt")
        log("starting the app")
        mark = logcat.mark()
        began = time.monotonic()
        d8.start_app(emulator, out, "first")
        served = logcat.wait_for(d8.SERVES, mark, 120)
        steps["korrid_port"] = int(served.group(1)) if served else None
        checks["korrid_loads_the_tls_verifiers_classes"] = logcat.wait_for(VERIFIER_LOADED, mark, 10) is not None
        log("waiting for Velvet to list the holder's game")
        shown = None
        until = time.monotonic() + args.library_seconds
        while time.monotonic() < until and shown is None:
            if sender is not None:
                # The sender's agent ends a session it has not heard from for 75 s.
                sender.request("clock")
            shown = logcat.wait_for(LIBRARY_LISTS_A_GAME, mark, 20)
        steps["seconds_to_the_game"] = round(time.monotonic() - began, 1)
        steps["library"] = shown.group(0) if shown else None
        time.sleep(3)
        # A newly bound device first shows korrid's "Identity connected" over
        # the library (surface/owner_binding.rs); its one action, "Open
        # library", takes Confirm, as a person would press it.
        # The first key that `input` injects after the start has been lost in
        # every run so far (README, known gaps): press again, and count.
        steps["identity_connected"] = d8.screenshot(emulator, out / "identity-connected.png")
        steps["confirm_presses"] = 0
        steps["changed_after_confirm"] = 0.0
        while steps["confirm_presses"] < 3 and steps["changed_after_confirm"] <= 0.05:
            emulator.shell("input keyevent KEYCODE_ENTER")
            steps["confirm_presses"] += 1
            time.sleep(4)
            steps["screenshot"] = d8.screenshot(emulator, out / "library.png")
            steps["changed_after_confirm"] = changed(out / "identity-connected.png", out / "library.png")
        steps["resumed"] = d8.focused(emulator)
        checks["velvet_lists_the_holders_game"] = shown is not None and steps["screenshot"]["colours"] > 8 \
            and steps["resumed"] == d8.ACTIVITY and steps["changed_after_confirm"] > 0.05
        peers = run_as(emulator, "cat", "no_backup/korrid-state/federation/peers.json").stdout
        checks["the_apps_korrid_remembers_the_holder"] = holder.key in peers

        crashes = [l for l in logcat.lines() if "FATAL" in l or "Fatal signal" in l or "korri: panic" in l
                   or "ANR in" in l]
        result["crashes"] = crashes
        checks["no_crash_panic_or_anr"] = not crashes
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
        log(f"failed: {result['error']}")
    finally:
        if logcat is not None:
            logcat.close()
            kept = [l for l in logcat.lines()
                    if " korri" in l or PACKAGE in l or "NativeActivity" in l or "FATAL" in l or "ANR" in l]
            (out / "logcat-app.txt").write_text("\n".join(kept) + "\n")
        result["emulator_stop"] = emulator.stop()
        subprocess.run([f"{sdk}/platform-tools/adb", "kill-server"], env=emulator.env, check=False,
                       capture_output=True, timeout=30)
        if holder is not None:
            stopped = holder.stop()
            events = stopped["events"] or []
            (out / "relay-events.json").write_text(json.dumps(events, indent=1) + "\n")
            device = result.get("member_device", "")
            owned = [e for e in events if e["pubkey"] == OWNER and tag(e, "d") == f"org.korri.device-owner:{device}"]
            sent = [e for e in events if e["pubkey"] == holder.key and tag(e, "d") == f"org.korri.endpoint:{device}"]
            checks["the_app_published_its_owner_statement_signed_by_the_test_owner"] = len(owned) == 1
            checks["the_holder_sent_the_app_its_endpoint"] = len(sent) >= 1
            checks["holder_joined_its_discovery"] = stopped["exit_code"] == 0
            for name in ("holder.log", "holder-ts.log"):
                if (scratch / name).is_file():
                    shutil.copy(scratch / name, out / name)
        if sender is not None:
            try:
                result["sender_restore"] = sender.close()
            except Exception as error:
                result["sender_restore_error"] = str(error)
        shutil.rmtree(scratch, ignore_errors=True)
    sender_restored = sender is None or result.get("sender_restore", {}).get("pass", False)
    result["pass"] = "error" not in result and bool(checks) and all(checks.values()) \
        and sender_restored and not result["emulator_stop"].get("processes_left")
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"pass": result["pass"], "checks": checks, "error": result.get("error")}, indent=2))
    log(f"record: {out}")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
