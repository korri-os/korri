#!/usr/bin/env nix
#! nix shell --inputs-from ../../.. nixpkgs#python3 nixpkgs#sway nixpkgs#grim --command python3
"""ADR 0086 proof: Velvet's Stage and Floor on two headless Sway outputs.

usage: panes.py SHELL FIXTURE_KORRID KORRID_TEST OUT_DIR [CYCLES]

SHELL and FIXTURE_KORRID are the release `korri-shell` and `fixture-korrid`
(`cargo build --release --features fixture --bins` in clients/shell).
KORRID_TEST is korrid's lib test binary (`cargo test --lib --no-run` in
services/korrid); its ignored test `live_pane_placement` is korrid's own
placement (`display_placement::lower_output`, `surface_windows`,
`place_surface`) run against this Sway. The fixture korrid publishes the
windows that placement printed, as korrid would.

Sway runs headless with the AYN Thor's arrangement: HEADLESS-1 1920x1080 at
0,0 (the main display) and HEADLESS-2 1240x1080 at 340,1080 under it.

It checks, and writes OUT_DIR/report.json and a PNG of each output:
  1. Velvet's scenarios (home, row, set, detail, settings, a QR page): the
     Stage window fullscreen on HEADLESS-1, the Floor's on HEADLESS-2.
  2. The fold: HEADLESS-2 disabled, both panes in one window on HEADLESS-1.
  3. Side by side: HEADLESS-2 beside the main display, both panes in one window.
  4. CYCLES (default 100) times: HEADLESS-2 disabled then enabled again, the
     panes folded and split, korrid placing each time; no crash.
"""
import json, os, re, shutil, subprocess, sys, tempfile, time

TOP, BOTTOM = "HEADLESS-1", "HEADLESS-2"
HERE = os.path.dirname(os.path.abspath(__file__))
PACKAGES = os.path.abspath(os.path.join(HERE, "..", "packages"))
SCENARIOS = os.path.join(PACKAGES, "velvet", "preview", "scenarios")
UNDER = [
    {"name": TOP, "main": True, "unwatched": False, "mode": "1920x1080@60Hz", "x": 0, "y": 0, "width": 1920, "height": 1080, "touch": None},
    {"name": BOTTOM, "main": False, "unwatched": False, "mode": "1240x1080@60Hz", "x": 340, "y": 1080, "width": 1240, "height": 1080, "touch": None},
]
BESIDE = [UNDER[0], dict(UNDER[1], x=1920, y=0)]
# The fixture korrid's gamepad: d-pad, south, east, select, start (clients/shell/src/input/mapper.rs).
CODES = {"up": "0x220", "down": "0x221", "left": "0x222", "right": "0x223", "confirm": "0x130", "back": "0x131", "options": "0x134", "menu": "0x13b"}


def libs():
    out = subprocess.run(
        ["nix", "build", "--no-link", "--print-out-paths", "--inputs-from", os.path.join(HERE, "..", "..", ".."),
         "nixpkgs#wayland", "nixpkgs#libglvnd", "nixpkgs#libxkbcommon", "nixpkgs#fontconfig.lib"],
        check=True, capture_output=True, text=True).stdout.split()
    return ":".join(p + "/lib" for p in out)


class Sway:
    def __init__(self, out):
        self.dir = tempfile.mkdtemp(prefix="panes-xdg-")
        os.chmod(self.dir, 0o700)
        config = os.path.join(self.dir, "config")
        with open(config, "w") as f:
            f.write(f"output {TOP} mode 1920x1080@60Hz pos 0 0\noutput {BOTTOM} mode 1240x1080@60Hz pos 340 1080\n"
                    "default_border none\nfocus_follows_mouse no\n")
        env = dict(os.environ, XDG_RUNTIME_DIR=self.dir, WLR_BACKENDS="headless", WLR_LIBINPUT_NO_DEVICES="1",
                   WLR_HEADLESS_OUTPUTS="2", WLR_RENDERER="gles2", WLR_RENDER_DRM_DEVICE="/dev/dri/renderD128")
        for name in ("WAYLAND_DISPLAY", "SWAYSOCK", "DISPLAY"):
            env.pop(name, None)
        self.log = open(os.path.join(out, "sway.log"), "a")
        self.proc = subprocess.Popen(["sway", "-c", config], env=env, stdout=self.log, stderr=self.log)
        for _ in range(100):
            socks = [s for s in os.listdir(self.dir) if s.startswith("sway-ipc")]
            ways = [s for s in os.listdir(self.dir) if re.fullmatch(r"wayland-\d+", s)]
            if socks and ways:
                break
            time.sleep(0.1)
        else:
            raise SystemExit("sway did not start")
        self.sock = os.path.join(self.dir, socks[0])
        self.wayland = ways[0]

    def msg(self, *args, t=None):
        cmd = ["swaymsg", "-s", self.sock, "-r"] + (["-t", t] if t else []) + list(args)
        r = subprocess.run(cmd, capture_output=True, text=True)
        return json.loads(r.stdout) if r.stdout.strip() else None

    def windows(self, pid):
        found = []

        def visit(node, output):
            if node.get("type") == "output":
                output = node["name"]
            if node.get("pid") == pid:
                found.append(dict(con_id=node["id"], name=node.get("name"), output=output,
                                  fullscreen=node.get("fullscreen_mode"), rect=node["rect"]))
            for child in node.get("nodes", []) + node.get("floating_nodes", []):
                visit(child, output)

        visit(self.msg(t="get_tree"), None)
        return found

    def env(self):
        return dict(os.environ, XDG_RUNTIME_DIR=self.dir, WAYLAND_DISPLAY=self.wayland)

    def shoot(self, output, path):
        subprocess.run(["grim", "-o", output, path], env=self.env(), check=True)

    def close(self):
        self.proc.terminate()
        try:
            self.proc.wait(5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
        shutil.rmtree(self.dir, ignore_errors=True)


class Run:
    """One fixture korrid and one shell showing Velvet."""

    def __init__(self, shell, fixture, sway, out, cache, model):
        self.sway, self.out = sway, out
        self.dir = tempfile.mkdtemp(prefix="panes-run-")
        os.makedirs(os.path.join(self.dir, "credentials"), mode=0o700)
        self.fixture = subprocess.Popen([fixture, "--games", "12", "--credentials", os.path.join(self.dir, "credentials")],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=open(os.path.join(out, "fixture.log"), "a"), text=True, bufsize=1)
        facts = {}
        while len(facts) < 2:
            line = self.fixture.stdout.readline()
            if not line:
                raise SystemExit("fixture korrid ended")
            if "=" in line:
                key, value = line.strip().split("=", 1)
                facts[key] = value
        if model is not None:
            path = os.path.join(self.dir, "model.json")
            json.dump(model, open(path, "w"))
            self.send(f"model {path}")
            time.sleep(0.2)
        env = dict(sway.env(), KORRID_ADDRESS=facts["KORRID_ADDRESS"], KORRID_SHELL_ORIGIN=facts["KORRID_SHELL_ORIGIN"],
                   CREDENTIALS_DIRECTORY=os.path.join(self.dir, "credentials"), LD_LIBRARY_PATH=libs(),
                   __EGL_VENDOR_LIBRARY_FILENAMES="/run/opengl-driver/share/glvnd/egl_vendor.d/50_mesa.json")
        env.pop("DISPLAY", None)
        self.shell = subprocess.Popen([shell, "--packages", PACKAGES, "--cache", cache, "--surfaces", "velvet"],
                                      env=env, stdout=open(os.path.join(out, "shell.log"), "a"), stderr=subprocess.STDOUT)
        self.until(lambda: any(w["name"] == "Velvet" for w in self.windows()), "the shell's window")
        self.send("pad")

    def send(self, line):
        self.fixture.stdin.write(line + "\n")
        self.fixture.stdin.flush()

    def windows(self):
        return self.sway.windows(self.shell.pid)

    def until(self, check, what, timeout=10.0):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if self.shell.poll() is not None:
                raise SystemExit(f"the shell ended (code {self.shell.returncode}) waiting for {what}")
            if check():
                return
            time.sleep(0.05)
        raise SystemExit(f"timed out waiting for {what}: {self.windows()}")

    def place(self, korrid_test, displays):
        """korrid's own placement; the fixture then publishes the windows it gave."""
        env = dict(os.environ, KORRI_LIVE_SWAYSOCK=self.sway.sock, KORRI_LIVE_SHELL_PID=str(self.shell.pid),
                   KORRI_LIVE_DISPLAYS=json.dumps(displays))
        r = subprocess.run([korrid_test, "host::display_placement::tests::live_pane_placement", "--ignored", "--exact", "--nocapture", "--test-threads=1"],
                           env=env, capture_output=True, text=True)
        match = re.search(r"WINDOWS (\S+)", r.stdout)
        if r.returncode != 0 or match is None:
            raise SystemExit(f"korrid placement failed: {r.stdout[-800:]}{r.stderr[-800:]}")
        windows = json.loads(match.group(1))
        self.send("windows " + json.dumps(windows) if windows and len(windows) > 1 else "windows none")
        return windows

    def split(self, korrid_test):
        windows = self.place(korrid_test, UNDER)
        assert len(windows) == 2, windows
        self.until(lambda: any(w["name"] == "korri-pane:floor" for w in self.windows()), "the Floor's window")
        # korrid places it on the declare after its first frame; the proof places again.
        self.place(korrid_test, UNDER)
        self.until(lambda: self.placed(split=True), "both windows placed")

    def placed(self, split):
        ws = self.windows()
        stage = [w for w in ws if w["name"] == "Velvet"]
        floor = [w for w in ws if w["name"] == "korri-pane:floor"]
        if len(stage) != 1 or stage[0]["output"] != TOP or not stage[0]["fullscreen"]:
            return False
        if split:
            return len(floor) == 1 and floor[0]["output"] == BOTTOM and bool(floor[0]["fullscreen"])
        return len(floor) == 0

    def press(self, step):
        self.send(f"press {CODES[step]}")
        time.sleep(0.35)

    def close(self):
        for proc in (self.shell, self.fixture):
            proc.terminate()
            try:
                proc.wait(5)
            except subprocess.TimeoutExpired:
                proc.kill()
        shutil.rmtree(self.dir, ignore_errors=True)


def main():
    shell, fixture, korrid_test, out = (os.path.abspath(a) for a in sys.argv[1:5])
    cycles = int(sys.argv[5]) if len(sys.argv) > 5 else 100
    os.makedirs(out, exist_ok=True)
    cache = tempfile.mkdtemp(prefix="panes-cache-")
    os.chmod(cache, 0o700)
    subprocess.run([shell, "install", "--packages", PACKAGES, "--cache", cache], check=True,
                   stdout=open(os.path.join(out, "install.log"), "w"), stderr=subprocess.STDOUT)
    report = {"scenarios": {}, "fold": None, "beside": None, "cycles": None}
    sway = Sway(out)
    try:
        shots = [("home", "home", []), ("row", "home", ["down"]), ("set", "set", None), ("detail", "detail", None),
                 ("settings", "settings", None), ("qr", "steam-code", None)]
        for name, scenario, steps in shots:
            spec = json.load(open(os.path.join(SCENARIOS, f"{scenario}.json")))
            run = Run(shell, fixture, sway, out, cache, spec["models"][0])
            try:
                run.split(korrid_test)
                time.sleep(1.0)
                for step in spec["steps"] if steps is None else steps:
                    if step.startswith("model ") or step.startswith("wait"):
                        continue
                    run.press(step)
                time.sleep(1.2)
                ok = run.placed(split=True)
                sway.shoot(TOP, os.path.join(out, f"{name}.stage.png"))
                sway.shoot(BOTTOM, os.path.join(out, f"{name}.floor.png"))
                report["scenarios"][name] = {"placed": ok, "windows": run.windows()}
                print(f"{name}: {'placed' if ok else 'NOT PLACED'}", flush=True)
            finally:
                run.close()

        run = Run(shell, fixture, sway, out, cache, None)
        try:
            run.split(korrid_test)
            # The fold: the display under the main one goes.
            sway.msg(f"output {BOTTOM} disable")
            windows = run.place(korrid_test, UNDER)
            run.until(lambda: run.placed(split=False), "the fold")
            time.sleep(1.0)
            sway.shoot(TOP, os.path.join(out, "fold.stage.png"))
            report["fold"] = {"windows": windows, "placed": run.placed(split=False)}
            print(f"fold: {windows}", flush=True)
            # Side by side: the display beside the main one is not under it.
            sway.msg(f"output {BOTTOM} enable; output {BOTTOM} pos 1920 0")
            active = [o["name"] for o in sway.msg(t="get_outputs") if o["active"]]
            windows = run.place(korrid_test, BESIDE)
            time.sleep(0.5)
            report["beside"] = {"windows": windows, "placed": run.placed(split=False), "active": active}
            print(f"beside: {windows}", flush=True)
            sway.msg(f"output {BOTTOM} pos 340 1080")
            # Cycles: disabled and enabled again, korrid placing each time.
            failures, times = [], []
            run.split(korrid_test)
            for i in range(cycles):
                started = time.monotonic()
                try:
                    sway.msg(f"output {BOTTOM} disable")
                    assert len(run.place(korrid_test, UNDER)) == 1
                    run.until(lambda: run.placed(split=False), f"fold {i}")
                    sway.msg(f"output {BOTTOM} enable")
                    run.split(korrid_test)
                except (AssertionError, SystemExit) as error:
                    failures.append(f"cycle {i}: {error}")
                    if run.shell.poll() is not None:
                        break
                times.append(time.monotonic() - started)
            times.sort()
            report["cycles"] = {"cycles": cycles, "failures": failures, "shell_alive": run.shell.poll() is None,
                                "p50_s": times[len(times) // 2] if times else None,
                                "p95_s": times[int(len(times) * 0.95)] if times else None}
            print(f"cycles: {len(failures)} failures of {cycles}; shell alive {run.shell.poll() is None}", flush=True)
        finally:
            run.close()
    finally:
        sway.close()
        shutil.rmtree(cache, ignore_errors=True)
        json.dump(report, open(os.path.join(out, "report.json"), "w"), indent=2)
    bad = [n for n, s in report["scenarios"].items() if not s["placed"]]
    bad += ["fold"] if not (report["fold"] or {}).get("placed") else []
    bad += ["beside"] if (report["beside"] or {}).get("windows") is None or len(report["beside"]["windows"]) != 1 else []
    bad += ["cycles"] if (report["cycles"] or {}).get("failures") or not (report["cycles"] or {}).get("shell_alive") else []
    print("PASS" if not bad else f"FAIL: {bad}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
