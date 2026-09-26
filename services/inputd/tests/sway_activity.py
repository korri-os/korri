"""Real Sway + swayidle boundary. Only this test uses a two-second timeout."""
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import tempfile
import time

processes = []
logs = []


def start(argv, **kwargs):
    child = subprocess.Popen(argv, **kwargs)
    processes.append(child)
    return child


def wait_for(predicate, message, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.025)
    raise AssertionError(message)


def ipc(command, kind="command"):
    # Speak the same native IPC as swaymsg, with bounded reads/timeouts.
    types = {"command": 0, "get_outputs": 3, "get_tree": 4}
    payload = command.encode()
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(2)
        sock.connect(os.environ["SWAYSOCK"])
        sock.sendall(b"i3-ipc" + len(payload).to_bytes(4, "little")
                     + types[kind].to_bytes(4, "little") + payload)

        def read(size):
            data = b""
            while len(data) < size:
                part = sock.recv(size - len(data))
                assert part, "IPC closed early"
                data += part
            return data

        header = read(14)
        assert header[:6] == b"i3-ipc"
        size = int.from_bytes(header[6:10], "little")
        assert size <= 1024 * 1024
        return json.loads(read(size))


def command(text):
    result = ipc(text)
    assert all(item.get("success") for item in result), (text, result)


def windows():
    result = []

    def walk(node):
        if node.get("app_id") == "wev":
            result.append((node["id"], node["focused"]))
        for child in node.get("nodes", []) + node.get("floating_nodes", []):
            walk(child)
    walk(ipc("", "get_tree"))
    return sorted(result)


def pointer_events():
    return [[line for line in path.read_text().splitlines() if "wl_pointer" in line]
            for path in logs]


def pointer_move(pointer):
    pointer.stdin.write(b"m")
    pointer.stdin.flush()
    assert select.select([pointer.stdout], [], [], 3)[0]
    assert pointer.stdout.readline() == b"moved\n"


with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    os.environ.update(XDG_RUNTIME_DIR=directory, SWAYSOCK=str(root / "sway.sock"),
                      WLR_BACKENDS="headless", WLR_RENDERER="pixman",
                      WLR_LIBINPUT_NO_DEVICES="1")
    config = root / "sway.conf"
    config.write_text("xwayland disable\noutput HEADLESS-1 mode 800x600\n"
                      "seat seat0 fallback true\nseat * hide_cursor 200\n")
    sway_log = root / "sway.log"
    try:
        with sway_log.open("w") as output:
            sway = start(["sway", "--unsupported-gpu", "-c", str(config)],
                         stdout=output, stderr=output)
        wait_for(lambda: (root / "sway.sock").exists(), "Sway did not start")
        wayland = wait_for(lambda: [p for p in root.glob("wayland-*") if p.is_socket()],
                           "Wayland socket missing")[0]
        os.environ["WAYLAND_DISPLAY"] = wayland.name
        # An unpatched compositor must fail here, before any positive-control input.
        command("seat seat0 idle_notify")
        assert not ipc("seat missing idle_notify")[0]["success"]
        assert not ipc("seat seat0 idle_notify unexpected")[0]["success"]
        pointer = start(["sway-test-pointer"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        assert select.select([pointer.stdout], [], [], 3)[0]
        assert pointer.stdout.readline() == b"ready\n"
        for i in range(2):
            log = root / f"wev-{i}.log"
            logs.append(log)
            with log.open("w") as output:
                start(["stdbuf", "-oL", "wev"], stdout=output, stderr=output)
            wait_for(lambda: len(windows()) == i + 1, "wev did not map")
        pointer_move(pointer)
        wait_for(lambda: any("enter" in line for lines in pointer_events() for line in lines),
                 "positive control did not deliver pointer input")
        wait_for(lambda: any("leave" in line for lines in pointer_events() for line in lines),
                 "cursor did not hide")
        command(f"[con_id={windows()[-1][0]}] focus")
        baseline_focus = windows()
        baseline_events = pointer_events()
        for _ in range(10):
            command("seat * idle_notify")
        time.sleep(0.3)  # Let unsolicited Wayland events drain into both observers.
        assert pointer_events() == baseline_events, "idle_notify emitted pointer/cursor events"
        assert windows() == baseline_focus, "idle_notify changed focus"

        events = root / "idle-events"
        events.touch()
        idle_log = root / "idle.log"
        with idle_log.open("w") as output:
            idle = start(["swayidle", "-w", "timeout", "2",
                          f"echo idle >> {events}",
                          "resume", f"echo awake >> {events}"],
                         stdout=output, stderr=output)
        wait_for(lambda: "idle" in events.read_text(), "native idle timeout did not fire")
        command("seat * idle_notify")
        wait_for(lambda: "awake" in events.read_text(), "activity did not resume swayidle")
        before = events.read_text()
        for _ in range(6):
            command("seat * idle_notify")
            time.sleep(0.5)
        assert events.read_text() == before, "continuous activity did not prevent idle"
        wait_for(lambda: events.read_text().count("idle") == 2, "idle did not resume after activity stopped")
        assert pointer_events() == baseline_events, "idle/wake produced pointer events"
        assert windows() == baseline_focus, "idle/wake changed focus"
        # Check that both observers still receive real pointer input afterwards.
        command("seat * idle_notify")
        wait_for(lambda: events.read_text().count("awake") == 2, "second wake failed")
        pointer_move(pointer)
        wait_for(lambda: pointer_events() != baseline_events, "pointer observer stopped working")
        assert sway.poll() is None and idle.poll() is None
        print("PASS: native idle reset, resume, sustained activity, re-idle; no pointer/focus effects")
        idle.terminate()
        idle.wait(timeout=3)

        # Separately exercise the product's real output-power commands. Sway's
        # existing DPMS power-on path rebases pointer focus, unlike idle_notify
        # itself. Do not mislabel these native output events as injected input.
        events.write_text("")
        with idle_log.open("a") as output:
            power_idle = start(["swayidle", "-w", "timeout", "2",
                                f"swaymsg 'output HEADLESS-1 power off'; echo idle >> {events}",
                                "resume", f"swaymsg 'output HEADLESS-1 power on'; echo awake >> {events}"],
                               stdout=output, stderr=output)
        wait_for(lambda: "idle" in events.read_text(), "headless output did not idle")
        assert not ipc("", "get_outputs")[0]["power"]
        command("seat * idle_notify")
        wait_for(lambda: "awake" in events.read_text(), "headless output did not resume")
        assert ipc("", "get_outputs")[0]["power"]
        assert windows() == baseline_focus, "output-power cycle changed window focus"
        assert sway.poll() is None and power_idle.poll() is None
        print("PASS: native swayidle output power-off and controller activity power-on")
    except BaseException:
        for path in [sway_log, *logs, root / "idle.log"]:
            if path.exists():
                print(f"--- {path.name} ---\n{path.read_text()}")
        raise
    finally:
        for child in reversed(processes):
            child.terminate()
        for child in reversed(processes):
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
