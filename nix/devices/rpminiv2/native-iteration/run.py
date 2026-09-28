#!/usr/bin/env python3
"""RAM-only component transaction and offline evidence gate. No builds or SSH."""
import argparse
import base64
import grp
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import pwd
import re
import shutil
import signal
import stat
import struct
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET

KIOSK = "korri-chromium-kiosk.service"
CORE = "korri-input-seat-receiver.service"
PROVIDER = "inputplumber.service"
INPUTD = "korri-inputd.service"
BRAIN = "korrid.service"
SUNSHINE = "korri-sunshine.service"
CONTROL = "korrid-control.socket"
CONTROL_PATH = Path("/run/korrid-control/control.sock")
KIOSK_GATE = f"/run/systemd/system/{KIOSK}.d/96-native-start-gate.conf"
ALLOW_TEXT = "native component trial kiosk start allowed\n"
VIEW = Path("/run/systemd/system/korri-chromium-kiosk.service.d/60-input-view.conf")
POLICY_DIRS = ("/run/current-system/sw/share/polkit-1/actions",
               "/etc/polkit-1/rules.d", "/run/current-system/sw/share/polkit-1/rules.d")
SYSTEM = "/nix/store/kij0kw2k0pz450ilnkpg27vmcm0dwiaw-nixos-system-rpminiv2-sd-card-26.05.20251221.a653104"
BOOT = "8679b6fb-2dd2-48a8-9805-231a56adfa57"
CID = "1d41445553440000200000365a019700"
BINARIES = ("inputplumber", "korri-inputd", "korri-input-seat-receiver", "korrid",
            "korri-portal-shell", "korri-replay-native-dpad")


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def command(*argv, timeout=25, check=True):
    # Never forward command output on failure: units/RPC can contain secrets.
    p = subprocess.run(list(map(str, argv)), stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=timeout, check=False)
    require(not check or p.returncode == 0, "required command failed (output private)")
    return p


def text(*argv, **kw):
    return command(*argv, **kw).stdout.decode().strip()


def show(unit, prop):
    return text("systemctl", "show", unit, "--property=" + prop, "--value")


def systemctl(verb, *units):
    command("systemctl", verb, *units, timeout=35)


def write(path, data, mode=0o600):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix="." + path.name + ".", dir=path.parent)
    temp = Path(name)
    try:
        with os.fdopen(fd, "w") as stream:
            stream.write(data)
            os.fchmod(stream.fileno(), mode)
        os.replace(temp, path)
    finally:
        if temp.exists():
            temp.unlink()


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def fingerprint(path):
    """Private comparison record, preserving symlinks, permissions and bytes."""
    path = Path(path)
    if not path.exists() and not path.is_symlink():
        return None
    st = path.lstat()
    value = {"mode": stat.S_IMODE(st.st_mode), "uid": st.st_uid, "gid": st.st_gid}
    if path.is_symlink():
        value["link"] = os.readlink(path)
    elif path.is_file():
        value["sha256"] = digest(path)
    elif path.is_dir():
        value["children"] = {p.name: fingerprint(p) for p in sorted(path.iterdir())}
    else:
        raise RuntimeError("snapshot requires ordinary files/directories/symlinks")
    return value


def guarded_path(value):
    path = Path(value)
    require(path.is_absolute() and not any(c.isspace() for c in value)
            and not any(c in value for c in '%:$\\"') and ".." not in path.parts,
            "unsafe operational path")
    return path


def assert_stage(stage):
    require(re.fullmatch(r"/run/korri-input-trial\.[A-Za-z0-9]{8}", str(stage)), "bad stage")
    require(os.geteuid() == 0 and not stage.is_symlink(), "root-owned stage required")
    for path, mode in [(stage, 0o755), (stage / "baseline", 0o700),
                       (stage / "control", 0o700), (stage / "evidence", 0o700)]:
        st = path.stat()
        require(st.st_uid == 0 and stat.S_IMODE(st.st_mode) == mode and not path.is_symlink(),
                "unsafe staging permissions")
    require(Path("/proc/device-tree/model").read_bytes().rstrip(b"\0") == b"Retroid Pocket Mini V2", "wrong model")
    require(text("findmnt", "-n", "-o", "SOURCE", "/") == "/dev/mmcblk0p2", "wrong root")
    require(text("findmnt", "-n", "-o", "SOURCE", "/boot") == "/dev/mmcblk0p1", "wrong boot disk")
    require(text("findmnt", "-n", "-o", "FSTYPE", "/run") == "tmpfs", "stage is not RAM")
    require("noexec" not in text("findmnt", "-n", "-o", "OPTIONS", "/run").split(","), "RAM stage is not executable")
    require(Path("/sys/class/block/mmcblk0/device/cid").read_text().strip() == CID, "wrong spare SD")
    require(str(Path("/run/current-system").resolve()) == SYSTEM, "system changed")
    require(Path("/proc/sys/kernel/random/boot_id").read_text().strip() == BOOT, "boot changed")


def payload_records(contents):
    seen = set()
    for line in contents.splitlines():
        match = re.fullmatch(r"([a-f0-9]{64})  ([A-Za-z0-9_./+-]+)", line)
        require(match is not None, "invalid payload checksum record")
        name = match[2]
        require(not name.startswith("/") and ".." not in Path(name).parts and name not in seen,
                "unsafe or duplicate payload path")
        require(name in ("run.py", "target.sh") or name.startswith(("bin/", "assets/", "inputs/")),
                "payload path outside immutable inputs")
        seen.add(name)
        yield name, match[1]


def verify_payload(stage):
    manifest = stage / "payload.sha256"
    require(not manifest.is_symlink() and manifest.stat().st_uid == 0
            and not manifest.stat().st_mode & 0o022, "unsafe checksum manifest")
    records = dict(payload_records(manifest.read_text()))
    for name, checksum in records.items():
        path = stage / name
        require(path.is_file() and not path.is_symlink() and digest(path) == checksum, "payload bytes changed")
        require(path.stat().st_uid == 0 and not path.stat().st_mode & 0o022, "unsafe payload owner/mode")
    actual = {str(p.relative_to(stage)) for root in ("bin", "assets", "inputs")
              for p in (stage / root).rglob("*") if not p.is_dir()}
    require(actual | {"run.py", "target.sh"} == set(records), "payload coverage incomplete")
    for root in ("bin", "assets", "inputs"):
        for path in [stage / root, *(stage / root).rglob("*")]:
            st = path.lstat()
            require(not path.is_symlink() and st.st_uid == 0 and not st.st_mode & 0o022,
                    "payload must be root owned without writable links")
            require(stat.S_IMODE(st.st_mode) == (0o755 if path.is_dir() or root == "bin" else 0o644),
                    "payload must be readable by sandboxed services")
    for name in BINARIES:
        check_elf(stage / "bin" / name)


def elf_interpreter(data):
    require(data[:6] == b"\x7fELF\x02\x01", "only little-endian ELF64 accepted")
    require(struct.unpack_from("<H", data, 18)[0] == 183, "binary is not aarch64")
    phoff = struct.unpack_from("<Q", data, 32)[0]
    size, count = struct.unpack_from("<HH", data, 54)
    require(size == 56 and 0 < count < 128, "invalid ELF program headers")
    for i in range(count):
        kind, _, offset, _, _, length, _, _ = struct.unpack_from("<IIQQQQQQ", data, phoff + i * size)
        if kind == 3:
            return data[offset:offset + length].rstrip(b"\0").decode()
    raise RuntimeError("expected dynamically linked prebuilt binary")


def check_elf(path):
    loader = elf_interpreter(path.read_bytes())
    require(loader.startswith("/nix/store/") and Path(loader).is_file(), "ELF interpreter absent")
    result = text(loader, "--list", path)
    require("not found" not in result, "RPATH dependency absent")
    libs = re.findall(r"(/nix/store/[^\s()]+)", result)
    require(libs and all(Path(lib).is_file() for lib in libs), "runtime libraries not resolved")


def idle_reply(value):
    return (isinstance(value, dict) and value.get("_tag") == "app.session.status"
            and isinstance(value.get("outcome"), dict)
            and value["outcome"].get("_tag") == "Err"
            and isinstance(value["outcome"].get("payload"), dict)
            and value["outcome"]["payload"].get("code") in ("NoActiveSession", "SessionCompleted"))


def no_game():
    data = text("runuser", "-u", "korri-inputd", "-g", "korri-control", "--",
                "curl", "--silent", "--show-error", "--max-time", "4", "--unix-socket",
                "/run/korrid-control/control.sock", "-H", "Content-Type: application/json",
                "--data", '{"_tag":"app.session.status","payload":{}}', "http://localhost/rpc", timeout=6)
    require(idle_reply(json.loads(data)), "session idle not proven; no destructive cutover")


def process_env(unit, key):
    pid = int(show(unit, "MainPID"))
    require(pid > 1, "baseline process absent")
    # Only requested whitelist value escapes this function. Never serialize environ.
    prefix = key.encode() + b"="
    values = [v[len(prefix):].decode() for v in Path(f"/proc/{pid}/environ").read_bytes().split(b"\0") if v.startswith(prefix)]
    require(len(values) == 1 and re.fullmatch(r"[/A-Za-z0-9_.:+-]+", values[0]), "required environment value missing/unsafe")
    return values[0]


def executable(unit):
    pid = int(show(unit, "MainPID"))
    require(pid > 1, "service process absent")
    return str(Path(f"/proc/{pid}/exe").resolve())


def socket_metadata(path):
    st = Path(path).stat()
    require(stat.S_ISSOCK(st.st_mode), "expected a real control socket")
    return {"uid": st.st_uid, "gid": st.st_gid, "mode": stat.S_IMODE(st.st_mode)}


def unit_snapshot(unit):
    info = {key: show(unit, key) for key in
            ("ActiveState", "SubState", "FragmentPath", "DropInPaths", "UnitFileState")}
    # Socket units have no MainPID. Capture their actual activation configuration.
    if unit == CONTROL:
        info.update({key: show(unit, key) for key in
                     ("Listen", "SocketUser", "SocketGroup", "SocketMode", "Triggers")})
        info["executable"] = None
    else:
        info["executable"] = executable(unit) if int(show(unit, "MainPID")) > 1 else None
    return info


def snapshot(stage, path):
    path = Path(path)
    target = stage / "baseline/files" / str(path).lstrip("/")
    target.parent.mkdir(parents=True, exist_ok=True)
    command("cp", "-a", "--", path, target)


def saved_path(stage, path):
    return stage / "baseline/files" / str(path).lstrip("/")


def save_state(stage, state):
    write(stage / "baseline/state.json", json.dumps(state, sort_keys=True))


def owned_install(stage, state, destination, source=None, mask=False):
    dest = Path(destination)
    require(str(dest).startswith(("/run/systemd/system/", "/run/udev/rules.d/")), "mutation outside runtime units/rules")
    require(not dest.exists() and not dest.is_symlink(), "runtime destination already exists")
    # Persist intended ownership before installation, covering interrupts.
    value = {"link": "/dev/null", "mode": 0o777, "uid": 0, "gid": 0} if mask else fingerprint(source)
    state["owned"][str(dest)] = value
    save_state(stage, state)
    dest.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    if mask:
        dest.symlink_to("/dev/null")
    else:
        os.link(source, dest)


def override(stage, state, unit, body):
    source = stage / "generated" / (unit + ".conf")
    source.parent.mkdir(mode=0o755, exist_ok=True)
    write(source, "[Service]\n" + body, 0o644)
    owned_install(stage, state, f"/run/systemd/system/{unit}.d/95-native-component-trial.conf", source)


def park(stage, state, path):
    path = Path(path)
    if not path.exists() and not path.is_symlink():
        return
    require(str(path) in state["files"], "park requires baseline snapshot")
    state["parked"].append(str(path))
    save_state(stage, state)
    target = stage / "baseline/parked" / str(path).lstrip("/")
    target.parent.mkdir(parents=True, exist_ok=True)
    os.rename(path, target)


def metadata_actions(rules):
    # Include the bare namespace prefix used by the existing action-user denial.
    return set(re.findall(r'"(org\.shadowblip\.[^"]*)"', rules))


def prepare(stage, args):
    assert_stage(stage)
    verify_payload(stage)
    require(not (stage / "baseline/state.json").exists(), "stage already used")
    require(re.fullmatch(r"[a-zA-Z0-9_-]+\.service", args.old_receiver)
            and args.old_receiver not in (CORE, SUNSHINE, KIOSK, INPUTD, PROVIDER, BRAIN), "bad old receiver unit")
    units = [KIOSK, SUNSHINE, BRAIN, INPUTD, args.old_receiver, PROVIDER, "polkit.service", "nginx.service", CONTROL]
    for path in (KIOSK_GATE, KIOSK_GATE.replace("/run/systemd/", "/etc/systemd/")):
        require(not Path(path).exists() and not Path(path).is_symlink(), "kiosk start gate collision")
    require(show(CORE, "LoadState") == "not-found", "core receiver already installed")
    require(not (Path("/etc/systemd/system") / CORE).exists(), "persistent core fragment present")
    no_game()
    require(show(KIOSK, "FreezerState") == "running", "kiosk frozen")
    require(args.old_receiver not in show(BRAIN, "Requires").split()
            and SUNSHINE not in show(BRAIN, "Requires").split(),
            "baseline coordinator cannot recover independently of masked signed producer")
    require(show(INPUTD, "StatusText") == "Ready", "baseline inputd not ready")
    state = {"units": {}, "files": {}, "parked": [], "owned": {}, "old_receiver": args.old_receiver,
             "mutated": False, "restored": False, "removed": [], "had_view": VIEW.exists()}
    paths = set()
    for unit in units:
        require(show(unit, "ActiveState") == "active", "baseline service not active")
        info = unit_snapshot(unit)
        state["units"][unit] = info
        paths.update([info["FragmentPath"], *info["DropInPaths"].split()])
        runtime = Path("/run/systemd/system") / unit
        if runtime.exists() or runtime.is_symlink():
            paths.add(str(runtime))
        require(not (Path("/etc/systemd/system") / (unit + ".d/95-native-component-trial.conf")).exists(), "persistent override name collision")
        require(not (Path("/run/systemd/system") / (unit + ".d/95-native-component-trial.conf")).exists(), "runtime override collision")
    state["control_socket"] = socket_metadata(CONTROL_PATH)
    require(state["control_socket"] == {"uid": 0, "gid": grp.getgrnam("korri-control").gr_gid, "mode": 0o660},
            "private RPC socket authority mismatch")
    require(BRAIN in state["units"][CONTROL]["Triggers"].split()
            and CONTROL in show(BRAIN, "Requires").split(), "private RPC activation relationship missing")
    # Effective fragments/drop-ins and private plugin/selector evidence, never logs.
    for value in [*args.guard_path, args.asset_root, args.nginx_config,
                  "/run/udev/rules.d/99-z-korri-sunshine-input.rules", *POLICY_DIRS]:
        paths.add(str(guarded_path(value)))
    require(any(p.startswith("/var/lib/korri-plugin-host/") for p in args.guard_path), "supply exact plugin receipts/approval/registry guard paths")
    for value in sorted(paths):
        require(value and Path(value).exists(), "baseline path absent")
        state["files"][value] = fingerprint(value)
        snapshot(stage, value)
    if state["had_view"]:
        require(not VIEW.is_symlink() and "# Managed by rpminiv2-input-iterate\n" in VIEW.read_text(), "unowned kiosk view")
    state["identities"] = {name: [pwd.getpwnam(name).pw_uid, pwd.getpwnam(name).pw_gid]
                           for name in ("korrid", "korri-inputd", "korri", "korri-portal")}
    state["seat_group"] = grp.getgrnam("korri-sunshine-input-seat").gr_gid
    require(state["seat_group"] == 980, "signed receiver group differs from generated unit")
    require(pwd.getpwnam("korri-inputd").pw_gid == grp.getgrnam("korri-control").gr_gid, "inputd group mismatch")
    provider_pid = int(show(PROVIDER, "MainPID"))
    provider_argv = Path(f"/proc/{provider_pid}/cmdline").read_bytes().rstrip(b"\0").split(b"\0")
    require([v.decode() for v in provider_argv[1:]] == args.provider_arg, "provider argv differs from observed process")
    for unit in (INPUTD, BRAIN):
        pid = int(show(unit, "MainPID"))
        require(len(Path(f"/proc/{pid}/cmdline").read_bytes().rstrip(b"\0").split(b"\0")) == 1,
                "component argv requires explicit inspection; refusing to drop arguments")
    state["provider_data"] = process_env(PROVIDER, "XDG_DATA_DIRS")
    state["profile"] = process_env(INPUTD, "KORRI_INPUTD_PROFILE_PATH")
    require(all(Path(p).is_dir() for p in state["provider_data"].split(":")) and Path(state["profile"]).is_file(), "baseline profile/data absent")
    state["screenshot"] = process_env(KIOSK, "XDG_RUNTIME_DIR") + "/native-input-diagnostic.jpg.b64"
    require(not Path(state["screenshot"]).exists(), "previous diagnostic screenshot present")
    state["asset_root"] = str(guarded_path(args.asset_root))
    state["nginx_config"] = str(guarded_path(args.nginx_config))
    launcher = args.kiosk_launcher
    require(launcher.startswith("/nix/store/") and Path(launcher).is_file(), "existing launcher required")
    require("path=" + launcher + " ;" in show(KIOSK, "ExecStart"), "launcher not effective ExecStart")
    original = Path(launcher).read_text()
    patched, count = re.subn(r"/nix/store/[a-z0-9]{32}-korri-portal-shell-[^/\s]+/bin/korri-portal-shell", str(stage / "bin/korri-portal-shell"), original)
    require(count == 1, "unsupported kiosk launcher; refuse bootstrap rewrite")
    generated = stage / "generated"
    generated.mkdir(mode=0o755)
    generated.chmod(0o755)  # Explicitly override the private evidence umask.
    write(generated / "kiosk", patched, 0o755)
    require(original.startswith("#!/nix/store/") and Path(original.splitlines()[0][2:]).is_file(), "launcher interpreter absent")
    require("argv[]=" + launcher + " ;" in show(KIOSK, "ExecStart"), "launcher has unpreserved arguments")
    receiver = (stage / "inputs/receiver.service").read_text()
    k_uid, k_gid = state["identities"]["korrid"]
    r_uid, r_gid = state["identities"]["korri"]
    suffix = f"--runtime-dir /run/korri-input-seat --control-uid {k_uid} --control-gid {k_gid} --sunshine-uid {r_uid} --sunshine-gid 980 --event-gid {r_gid}"
    receiver, count = re.subn(r"(?m)^ExecStart=\S+/bin/korri-bundle-launch input-seat-receiver " + re.escape(suffix) + r"$", "ExecStart=" + str(stage / "bin/korri-input-seat-receiver") + " " + suffix, receiver)
    require(count == 1 and "User=root\n" in receiver and "Group=root\n" in receiver, "receiver generated unit identity/ExecStart mismatch")
    write(generated / CORE, receiver, 0o644)
    seat_rules = (stage / "inputs/seat.rules").read_text()
    tools = re.findall(r'RUN\+="(/nix/store/[a-z0-9]{32}-coreutils-[^/]+)/bin/chgrp ', seat_rules)
    require(len(tools) == 255 and len(set(tools)) == 1, "seat rules are not generated production rules")
    tool = tools[0]
    require(Path(tool + "/bin/chgrp").is_file() and Path(tool + "/bin/chmod").is_file(), "udev runtime helpers missing")
    expected_rules = "".join(
        f'ACTION=="add|change", SUBSYSTEM=="input", KERNEL=="event*", ATTRS{{name}}=="Korri Seat P{slot}", '
        f'ATTRS{{phys}}=="korri/input-seat/p{slot}", ATTRS{{id/bustype}}=="0003", '
        'ATTRS{id/vendor}=="045e", ATTRS{id/product}=="028e", ATTRS{id/version}=="0001", '
        f'TAG-="uaccess", OWNER="root", MODE="0600", RUN+="{tool}/bin/chgrp {r_gid} $env{{DEVNAME}}", '
        f'RUN+="{tool}/bin/chmod 0660 $env{{DEVNAME}}"\n' for slot in range(1, 256))
    require(seat_rules == expected_rules, "seat descriptor/GID/rules differ from exact producer")
    require(not Path("/run/udev/rules.d/99-z-korri-input-seat.rules").exists(), "core rules already present")
    baseline_nginx = Path(args.nginx_config).read_text()
    candidate_nginx = (stage / "inputs/nginx.conf").read_text()
    patched_nginx, count = re.subn(r"connect-src (http://127\.0\.0\.1:([0-9]+));", r"connect-src \1 ws://127.0.0.1:\2;", baseline_nginx)
    require(count == 1 and candidate_nginx == patched_nginx, "nginx change is not exact same-port HTTP+WS CSP delta")
    require(str(args.asset_root) in baseline_nginx, "asset root absent from effective nginx config")
    # Complete directory copies in the polkit unit namespace preserve unrelated policy.
    policy = stage / "policy"
    policy.mkdir(mode=0o755)
    policy.chmod(0o755)
    state["policy_contents"] = {}
    for i, directory in enumerate(POLICY_DIRS):
        # Save full effective directory contents privately, not only symlinks.
        private = stage / "baseline/policy" / str(i)
        private.parent.mkdir(mode=0o700, exist_ok=True)
        command("cp", "-aL", directory, private)
        state["policy_contents"][directory] = fingerprint(private)
        command("cp", "-a", private, policy / str(i))
    xml = stage / "inputs/InputPlumber.policy"
    actions = ET.parse(xml).getroot().findall("action[@id='org.shadowblip.Input.Target.DevicePaths']")
    require(len(actions) == 1, "protected action missing or duplicated")
    defaults = actions[0].find("defaults")
    require(defaults is not None and {child.tag: child.text for child in defaults}
            == {name: "no" for name in ("allow_any", "allow_inactive", "allow_active")},
            "protected metadata action defaults are not deny-all")
    action = policy / "0/org.shadowblip.InputPlumber.policy"
    require(action.is_file(), "baseline InputPlumber action absent")
    shutil.copyfile(xml, action)
    rules = stage / "inputs/metadata.rules"
    action_user = show(INPUTD, "User")  # inputd account remains fixed by existing RPC consumer.
    require(action_user == "korri-inputd", "unexpected inputd identity")
    expected_actions = {"org.shadowblip.Input.CompositeDevice." + name for name in
                        ("DbusDevices", "ProfilePath", "SourceDevicePaths", "TargetDevices", "LoadProfilePath", "Stop")}
    expected_actions.add("org.shadowblip.Input.Target.DevicePaths")
    require(metadata_actions(rules.read_text()) == expected_actions | {"org.shadowblip."},
            "metadata grants differ from source")
    expected_rules = '''polkit.addRule(function(action, subject) {
      if (subject.user == "korri" && action.id.indexOf("org.shadowblip.") == 0) {
        return polkit.Result.NO;
      }
      if (subject.user == "korri-inputd" && [
        "org.shadowblip.Input.CompositeDevice.DbusDevices",
        "org.shadowblip.Input.CompositeDevice.ProfilePath",
        "org.shadowblip.Input.CompositeDevice.SourceDevicePaths",
        "org.shadowblip.Input.CompositeDevice.TargetDevices",
        "org.shadowblip.Input.Target.DevicePaths",
        "org.shadowblip.Input.CompositeDevice.LoadProfilePath",
        "org.shadowblip.Input.CompositeDevice.Stop"
      ].indexOf(action.id) >= 0) {
        return polkit.Result.YES;
      }
    });'''
    require(" ".join(rules.read_text().split()) == " ".join(expected_rules.split()),
            "policy differs from exact source grants and action-user denial")
    require(process_env(INPUTD, "KORRI_INPUTD_ACTION_UID") == str(state["identities"]["korri"][0]),
            "source action-user identity does not match running inputd")
    destination = policy / "1/00-native-component-trial.rules"
    require(not destination.exists(), "policy rule collision")
    shutil.copyfile(rules, destination)
    destination.chmod(0o644)
    # Keep unrelated policy owners/modes exactly as copied. Only the new public
    # native rule gets an explicit readable mode; private baseline stays 0700.
    state["provider_args"] = args.provider_arg
    require(all(re.fullmatch(r"[A-Za-z0-9_./=-]+", s) for s in args.provider_arg), "unsafe provider argv")
    save_state(stage, state)
    return state


def wait_ready(unit, seconds=15):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if show(unit, "ActiveState") == "active":
            return
        time.sleep(0.25)
    raise RuntimeError("service readiness timeout")


def start(unit):
    systemctl("start", unit)
    wait_ready(unit)


def wait_idle_ready(expected_executable, seconds=15):
    """Transport startup can lag ActiveState. A real non-idle reply never can."""
    pid = show(BRAIN, "MainPID")
    require(pid.isdigit() and int(pid) > 1, "coordinator PID absent")
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        require(show(BRAIN, "ActiveState") == "active" and show(BRAIN, "MainPID") == pid,
                "coordinator changed during readiness")
        # The configured bundle launcher execs the daemon in the same PID.
        if executable(BRAIN) != expected_executable:
            time.sleep(0.25)
            continue
        result = command("runuser", "-u", "korri-inputd", "-g", "korri-control", "--",
                         "curl", "--silent", "--show-error", "--max-time", "4", "--unix-socket",
                         "/run/korrid-control/control.sock", "-H", "Content-Type: application/json",
                         "--data", '{"_tag":"app.session.status","payload":{}}',
                         "http://localhost/rpc", timeout=6, check=False)
        if result.returncode == 0:
            require(idle_reply(json.loads(result.stdout)), "coordinator is not proven idle")
            require(show(BRAIN, "MainPID") == pid and executable(BRAIN) == expected_executable,
                    "coordinator changed after readiness")
            return
        time.sleep(0.25)
    raise RuntimeError("coordinator RPC readiness timeout")


def wait_input_ready(seconds=15):
    pid = show(INPUTD, "MainPID")
    require(pid.isdigit() and int(pid) > 1, "inputd PID absent")
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        require(show(INPUTD, "ActiveState") == "active" and show(INPUTD, "MainPID") == pid,
                "inputd changed during readiness")
        if show(INPUTD, "StatusText") == "Ready":
            return
        time.sleep(0.25)
    raise RuntimeError("inputd capture readiness timeout")


def close_kiosk_gate(stage):
    # The private allow file is the only gate switch; preserve the drop-in until
    # every restored producer and the refreshed view are ready.
    remove_owned(stage / "control/kiosk-allow", {
        "uid": 0, "gid": 0, "mode": 0o600,
        "sha256": hashlib.sha256(ALLOW_TEXT.encode()).hexdigest()})


def install_kiosk_gate(stage, state):
    close_kiosk_gate(stage)
    source = stage / "generated/kiosk-start-gate.conf"
    if KIOSK_GATE not in state["owned"] or KIOSK_GATE in state["removed"]:
        write(source, f"[Unit]\nConditionPathExists={stage}/control/kiosk-allow\n", 0o644)
        owned_install(stage, state, KIOSK_GATE, source)
        if KIOSK_GATE in state["removed"]:
            state["removed"].remove(KIOSK_GATE)
            save_state(stage, state)
    else:
        actual = fingerprint(KIOSK_GATE)
        if actual is None:  # Interrupted between ownership record and link.
            require(fingerprint(source) == state["owned"][KIOSK_GATE], "kiosk gate source changed")
            os.link(source, KIOSK_GATE)
        else:
            require(actual == state["owned"][KIOSK_GATE], "kiosk gate changed")
    systemctl("daemon-reload")
    require(KIOSK_GATE in show(KIOSK, "DropInPaths").split(), "kiosk gate not effective")


def open_candidate_kiosk(stage):
    require(show(KIOSK, "ActiveState") == "inactive", "kiosk autostart bypassed gate")
    write(stage / "control/kiosk-allow", ALLOW_TEXT)
    start(KIOSK)


def start_brain(state, expected_executable):
    require(show(BRAIN, "ActiveState") == "inactive", "coordinator already running before explicit start")
    start(CONTROL)
    require(socket_metadata(CONTROL_PATH) == state["control_socket"], "private RPC socket metadata changed")
    require(show(BRAIN, "ActiveState") == "inactive", "unexpected socket activation before coordinator start")
    start(BRAIN)
    wait_idle_ready(expected_executable)
    # Verify the real listener FD reached the process, not merely a pathname or
    # LISTEN_FDS environment value. Read only the exact private socket's inode.
    pid = show(BRAIN, "MainPID")
    inodes = {line.split()[6] for line in Path("/proc/net/unix").read_text().splitlines()[1:]
              if len(line.split()) == 8 and line.split()[7] == str(CONTROL_PATH)}
    handles = set()
    for fd in Path(f"/proc/{pid}/fd").iterdir():
        try:
            handles.add(os.readlink(fd))
        except FileNotFoundError:
            pass
    require(any(f"socket:[{inode}]" in handles for inode in inodes), "coordinator lacks socket activation listener FD")
    require(show(BRAIN, "MainPID") == pid and executable(BRAIN) == expected_executable,
            "coordinator changed after activation FD check")


def stop_order(state):
    # Inputd polls the socket. Stop all clients before closing activation, then
    # the daemon; otherwise a status poll can resurrect the old executable.
    for unit in (INPUTD, KIOSK, SUNSHINE, CONTROL, BRAIN, state["old_receiver"], PROVIDER):
        systemctl("stop", unit)
        require(show(unit, "ActiveState") == "inactive", "service failed to stop cleanly")


def trial(stage, args):
    state = prepare(stage, args)
    no_game()
    state["mutated"] = True
    save_state(stage, state)
    install_kiosk_gate(stage, state)  # Parent Wants/PartOf can start the kiosk.
    stop_order(state)  # Signed receiver cleanup finishes BEFORE core rules exist.
    for unit in (SUNSHINE, state["old_receiver"]):
        runtime = Path("/run/systemd/system") / unit
        park(stage, state, runtime)
        owned_install(stage, state, runtime, mask=True)
    park(stage, state, VIEW)
    owned_install(stage, state, "/run/systemd/system/" + CORE, stage / "generated" / CORE)
    rules = "/run/udev/rules.d/99-z-korri-input-seat.rules"
    owned_install(stage, state, rules, stage / "inputs/seat.rules")
    override(stage, state, PROVIDER, "ExecStart=\nExecStart=" + str(stage / "bin/inputplumber") + " " + " ".join(state["provider_args"]) + '\nEnvironment="XDG_DATA_DIRS=' + state["provider_data"] + '"\n')
    override(stage, state, INPUTD, "ExecStart=\nExecStart=" + str(stage / "bin/korri-inputd") + '\nEnvironment="KORRI_INPUTD_PROFILE_PATH=' + state["profile"] + '"\n')
    override(stage, state, BRAIN, "ExecStart=\nExecStart=" + str(stage / "bin/korrid") + "\n")
    override(stage, state, KIOSK, "ExecStart=\nExecStart=" + str(stage / "generated/kiosk") + "\n")
    override(stage, state, "polkit.service", "".join(f"BindReadOnlyPaths={stage}/policy/{i}:{directory}\n" for i, directory in enumerate(POLICY_DIRS)))
    override(stage, state, "nginx.service", f"BindReadOnlyPaths={stage}/assets:{state['asset_root']}\nBindReadOnlyPaths={stage}/inputs/nginx.conf:{state['nginx_config']}\n")
    systemctl("daemon-reload")
    for unit in (SUNSHINE, state["old_receiver"]):
        require(show(unit, "LoadState") == "masked" and show(unit, "ActiveState") == "inactive",
                "runtime mask ineffective; refusing to start a second seat authority")
    command("udevadm", "control", "--reload-rules")
    command("udevadm", "trigger", "--subsystem-match=input", "--action=change")
    command("udevadm", "settle", "--timeout=10")
    systemctl("restart", "polkit.service")
    command("pkaction", "--action-id", "org.shadowblip.Input.Target.DevicePaths")
    for unit in (PROVIDER, CORE):
        start(unit)
    start_brain(state, str(stage / "bin/korrid"))
    start(INPUTD)
    wait_input_ready()
    sock = Path("/run/korri-input-seat/control.sock").stat()
    require(stat.S_ISSOCK(sock.st_mode) and sock.st_uid == 0
            and sock.st_gid == state["identities"]["korrid"][1]
            and stat.S_IMODE(sock.st_mode) == 0o660, "receiver socket authority mismatch")
    for unit, binary in ((PROVIDER, "inputplumber"), (CORE, "korri-input-seat-receiver"), (BRAIN, "korrid"), (INPUTD, "korri-inputd")):
        require(executable(unit) == str(stage / "bin" / binary), "candidate binary not active")
    for unit in (SUNSHINE, state["old_receiver"]):
        require(show(unit, "ActiveState") == "inactive" and show(unit, "LoadState") == "masked", "old producer not isolated")
    no_game()
    systemctl("restart", "nginx.service")
    open_candidate_kiosk(stage)
    candidate_exes = {unit: str(stage / "bin" / binary) for unit, binary in
                      ((PROVIDER, "inputplumber"), (CORE, "korri-input-seat-receiver"),
                       (BRAIN, "korrid"), (INPUTD, "korri-inputd"), (KIOSK, "korri-portal-shell"))}
    identities = process_identities(candidate_exes)
    time.sleep(0.5)
    require(process_identities(candidate_exes) == identities, "candidate processes changed after startup")
    require(show(KIOSK, "FreezerState") == "running", "candidate kiosk frozen")
    invocation = show(KIOSK, "InvocationID")
    require(re.fullmatch(r"[a-f0-9]{32}", invocation), "invalid kiosk invocation")
    write(stage / "control/ready", "READY: supply protected physical composite/target/event tuple; no games\n")
    if args.check_rollback:
        os.kill(os.getpid(), signal.SIGTERM)
    end = time.monotonic() + 45
    replay_input = stage / "control/replay-target"
    while not replay_input.exists() and time.monotonic() < end:
        time.sleep(0.25)
    require(replay_input.is_file() and replay_input.stat().st_uid == 0 and not replay_input.is_symlink(), "no protected target handoff")
    values = replay_input.read_text().splitlines()
    require(len(values) == 3 and all(re.fullmatch(r"/[A-Za-z0-9_/]+", s) for s in values), "invalid replay tuple")
    require(re.fullmatch(r"/dev/input/event[0-9]+", values[2]), "not an evdev event node")
    # Metadata authorization must work as inputd and must fail as its action user.
    prop = ["busctl", "--system", "--timeout=2", "get-property", "org.shadowblip.InputPlumber", values[1], "org.shadowblip.Input.Target", "DevicePaths"]
    command("runuser", "-u", "korri-inputd", "-g", "korri-control", "--", *prop)
    require(command("runuser", "-u", "korri", "--", *prop, check=False).returncode != 0, "action user can read protected metadata")
    no_game()
    time.sleep(2)
    replay = command(stage / "bin/korri-replay-native-dpad", *values, timeout=17, check=False)
    write(stage / "evidence/replay.log", (replay.stdout + replay.stderr).decode())
    require(replay.returncode == 0, "native replay failed; delivery not proven")
    # Probe screenshot is emitted >=15 seconds into the kiosk invocation.
    deadline = time.monotonic() + 20
    while not Path(state["screenshot"]).is_file() and time.monotonic() < deadline:
        time.sleep(0.25)
    screenshot = Path(state["screenshot"])
    require(screenshot.is_file() and screenshot.stat().st_size <= 900000, "private screenshot missing/oversize")
    shutil.copyfile(screenshot, stage / "evidence/screen.b64")
    state["screenshot_fingerprint"] = fingerprint(screenshot)
    save_state(stage, state)
    records = text("journalctl", "-b", "_SYSTEMD_INVOCATION_ID=" + invocation,
                   "--grep=^\\[DEBUG-native-input\\]", "-o", "cat", "--no-pager", "--quiet")
    require(0 < len(records) < 1000000, "observer output missing/oversize")
    write(stage / "evidence/observer.log", records + "\n")
    no_game()
    write(stage / "evidence/capture", "CAPTURE COMPLETE: physical native path only; host verdict required\n")


def view_text():
    # Same owned view algorithm as iteration/keep.sh, but resolve NEW sysfs parents.
    portal = [p.parent.resolve() for p in Path("/sys/class/input").glob("event*/device/phys")
              if p.read_text().strip() == "korri/inputd/portal"]
    require(len(set(portal)) == 1, "restored portal target is not unique")
    parents = set()
    for node in Path("/sys/class/input").glob("js*"):
        parent = (node / "device").resolve()
        if re.fullmatch(r"/sys/devices/virtual/input/input[0-9]+", str(parent)) and parent != portal[0]:
            parents.add(str(parent))
    return ("# Managed by rpminiv2-input-iterate\n[Service]\nTemporaryFileSystem=\n"
            + "".join(f"TemporaryFileSystem={p}:ro,mode=755\n" for p in sorted(parents)))


def process_identities(expected):
    result = {}
    for unit, exe in expected.items():
        require(unit != CONTROL, "socket units have no process identity")
        pid = show(unit, "MainPID")
        require(show(unit, "ActiveState") == "active" and pid.isdigit() and int(pid) > 1,
                "required producer process absent")
        actual = executable(unit)
        require(actual == exe and show(unit, "MainPID") == pid, "producer executable/PID changed")
        result[unit] = [pid, actual]
    return result


def settled_view(expected, seconds=10, quiet=2):
    """Require the complete view AND producer PIDs to stay stable; omit no pad."""
    deadline = time.monotonic() + seconds
    previous, since = None, time.monotonic()
    while time.monotonic() < deadline:
        try:
            identities = process_identities(expected)
            current = (view_text(), identities)
        except (RuntimeError, OSError):
            previous, since = None, time.monotonic()
            time.sleep(0.5)
            continue
        now = time.monotonic()
        if current != previous:
            previous, since = current, now
        elif now - since >= quiet:
            return current
        time.sleep(0.5)
    raise RuntimeError("restored producer topology did not settle")


def remove_owned(path, expected):
    actual = fingerprint(path)
    require(actual is None or actual == expected, "trial-owned file changed; refusing removal")
    if actual is not None:
        Path(path).unlink()


def restore(stage):
    assert_stage(stage)
    state = json.loads((stage / "baseline/state.json").read_text())
    if state["restored"] or not state["mutated"]:
        verify(stage)
        return
    install_kiosk_gate(stage, state)
    screenshot = Path(state["screenshot"])
    # This diagnostic uses create-new. It was absent before the kiosk started.
    # Do not remove an unobserved file after a partial failure.
    if state.get("screenshot_fingerprint"):
        remove_owned(screenshot, state["screenshot_fingerprint"])
    # An unavailable candidate is NOT idle. Recover only the original coordinator
    # first, with kiosk/Sunshine stopped, then require its exact terminal RPC reply
    # before touching the input stack. No original receiver may start here.
    if show(BRAIN, "ActiveState") != "active":
        for unit in (INPUTD, KIOSK, SUNSHINE, CONTROL, BRAIN):
            systemctl("stop", unit)
        brain_override = f"/run/systemd/system/{BRAIN}.d/95-native-component-trial.conf"
        if brain_override in state["owned"] and brain_override not in state["removed"]:
            remove_owned(brain_override, state["owned"][brain_override])
            state["removed"].append(brain_override)
            save_state(stage, state)
        systemctl("daemon-reload")
        start_brain(state, state["units"][BRAIN]["executable"])
    no_game()
    for unit in (INPUTD, KIOSK, SUNSHINE, CONTROL, BRAIN, CORE, state["old_receiver"], PROVIDER):
        if show(unit, "LoadState") not in ("not-found", "masked"):
            systemctl("stop", unit)
    for path, expected in state["owned"].items():
        if path in state["removed"] or path == KIOSK_GATE:
            continue
        remove_owned(path, expected)
        state["removed"].append(path)
        save_state(stage, state)
    for path in state["parked"]:
        if path == str(VIEW):
            continue  # Never restore stale sysfs parents.
        parked = stage / "baseline/parked" / path.lstrip("/")
        if parked.exists() or parked.is_symlink():
            require(not Path(path).exists() and not Path(path).is_symlink(), "parked runtime destination occupied")
            os.rename(parked, path)
    systemctl("daemon-reload")
    command("systemctl", "reset-failed", CORE, check=False)
    command("udevadm", "control", "--reload-rules")
    systemctl("restart", "polkit.service")
    start(PROVIDER)
    # Original signed setup restores its own group/rules; no receipt/approval edit.
    start(state["old_receiver"])
    start_brain(state, state["units"][BRAIN]["executable"])
    start(INPUTD)
    wait_input_ready()
    no_game()
    start(SUNSHINE)  # Its absolute mouse creates another js node after startup.
    systemctl("restart", "nginx.service")  # Its kiosk Wants is still gated.
    command("udevadm", "settle", "--timeout=10")
    producers = {unit: state["units"][unit]["executable"]
                 for unit in (PROVIDER, state["old_receiver"], BRAIN, INPUTD, SUNSHINE)}
    current_topology, identities = settled_view(producers)
    state["restored_producers"] = identities
    if state["had_view"]:
        if VIEW.exists():
            require(fingerprint(VIEW) in (state.get("refreshed_view"), state["files"].get(str(VIEW))),
                    "owned view destination occupied")
        current_view = stage / "generated/restored-view.conf"
        if current_view.exists():
            current_view.unlink()
        write(current_view, current_topology, 0o644)
        state["refreshed_view"] = fingerprint(current_view)
        save_state(stage, state)
        os.replace(current_view, VIEW)
    systemctl("daemon-reload")
    no_game()
    require(show(KIOSK, "ActiveState") == "inactive", "restored kiosk bypassed closed start gate")
    # Recheck after the filesystem/unit update before releasing automatic starts.
    require(settled_view(producers) == (current_topology, identities), "topology changed before kiosk release")
    remove_owned(KIOSK_GATE, state["owned"][KIOSK_GATE])
    state["removed"].append(KIOSK_GATE)
    save_state(stage, state)
    systemctl("daemon-reload")
    start(KIOSK)
    require({name: [pwd.getpwnam(name).pw_uid, pwd.getpwnam(name).pw_gid]
             for name in state["identities"]} == state["identities"], "account identities changed")
    state["restored"] = True
    save_state(stage, state)
    verify(stage)
    write(stage / "evidence/restore.log", "RESTORED: baseline components, signed receiver, private policy and refreshed input view\n")


def verify(stage):
    assert_stage(stage)
    state = json.loads((stage / "baseline/state.json").read_text())
    for path, expected in state["files"].items():
        if path == str(VIEW) and state.get("refreshed_view"):
            require(VIEW.read_text() == view_text(), "restored input view is stale")
            expected = state["refreshed_view"]
        require(fingerprint(path) == expected, "baseline bytes/link/permissions changed")
    for directory, expected in state["policy_contents"].items():
        # Dereference policy symlinks exactly as during baseline capture.
        check_dir = stage / "baseline/policy-check"
        if check_dir.exists():
            shutil.rmtree(check_dir)
        command("cp", "-aL", directory, check_dir)
        require(fingerprint(check_dir) == expected, "effective baseline policy changed")
        shutil.rmtree(check_dir)
    require(socket_metadata(CONTROL_PATH) == state["control_socket"], "restored private RPC socket metadata differs")
    for unit, info in state["units"].items():
        for key in info:
            if key != "executable":
                require(show(unit, key) == info[key], "restored service state/authority differs")
        if info["executable"]:
            require(executable(unit) == info["executable"], "restored executable differs")
    for path in state["owned"]:
        if path not in state["parked"]:
            require(not Path(path).exists() and not Path(path).is_symlink(), "candidate runtime file remains")
    require(show(CORE, "ActiveState") == "inactive", "core receiver still active")
    require(grp.getgrnam("korri-sunshine-input-seat").gr_gid == state["seat_group"], "signed group not restored")
    require({name: [pwd.getpwnam(name).pw_uid, pwd.getpwnam(name).pw_gid]
             for name in state["identities"]} == state["identities"], "account identities changed")
    require(show(INPUTD, "StatusText") == "Ready" and show(KIOSK, "FreezerState") == "running", "baseline not usable")
    require(process_env(PROVIDER, "XDG_DATA_DIRS") == state["provider_data"], "provider data changed")
    require(process_env(INPUTD, "KORRI_INPUTD_PROFILE_PATH") == state["profile"], "inputd profile changed")
    no_game()
    expected = {unit: info["executable"] for unit, info in state["units"].items() if info["executable"]}
    topology, identities = settled_view(expected)
    if state["had_view"]:
        require(VIEW.read_text() == topology, "restored input view is stale after producer sampling")
    for unit, identity in state.get("restored_producers", {}).items():
        require(identities[unit] == identity, "restored producer restarted after topology capture")
    write(stage / "evidence/verification.log", "ROLLBACK VERIFIED: fresh service, byte, identity, policy, topology and idle checks\n")


def host_verdict(evidence, parser_path):
    for name, prefix in (("restore.log", "RESTORED:"), ("verification.log", "ROLLBACK VERIFIED:"),
                         ("capture", "CAPTURE COMPLETE:")):
        require((evidence / name).read_text().startswith(prefix), "capture/rollback evidence incomplete")
    data = (evidence / "screen.b64").read_bytes()
    require(0 < len(data) <= 900000, "screenshot missing/oversize")
    image = base64.b64decode(data, validate=True)
    require(image.startswith(b"\xff\xd8\xff") and image.endswith(b"\xff\xd9"), "invalid JPEG screenshot")
    spec = importlib.util.spec_from_file_location("native_verdict", parser_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    samples = module.observations(module.read_bounded(evidence / "observer.log", 1000000))
    start_us, end_us = module.replay_window(module.read_bounded(evidence / "replay.log", 8192))
    passed, reason = module.verdict(samples, start_us, end_us)
    require(passed, reason)
    return "PASS: physical native samples and DOM focus; rollback independently checked. NOT remote Sunshine acceptance."


def readiness_diagnostics(stage, phase):
    """Best-effort, three-second failure snapshot; no unit text/env/journal/RPC."""
    deadline = time.monotonic() + 3
    records = {}
    for unit in (BRAIN, INPUTD, CONTROL, KIOSK, CORE, PROVIDER, SUNSHINE, "nginx.service"):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        fields = ["ActiveState", "SubState", "Result"] + ([] if unit == CONTROL else ["MainPID"])
        try:
            result = command("systemctl", "show", unit, *["--property=" + f for f in fields],
                             timeout=remaining, check=False)
            record = {}
            for line in result.stdout.decode().splitlines():
                key, _, value = line.partition("=")
                if key in fields and re.fullmatch(r"[A-Za-z0-9_-]{1,40}", value):
                    record[key] = value
            pid = record.get("MainPID", "")
            if pid.isdigit() and int(pid) > 1:
                exe = os.readlink(f"/proc/{pid}/exe")
                if re.fullmatch(r"/(?:nix/store|run/korri-input-trial\.[A-Za-z0-9]{8})/[A-Za-z0-9_./+-]+", exe):
                    record["executable"] = exe
            records[unit] = record
        except (OSError, ValueError, subprocess.TimeoutExpired):
            continue
    write(stage / "evidence" / ("readiness-" + phase + ".json"), json.dumps(records, sort_keys=True))


def failure_diagnostics(args):
    if args.mode != "verdict":
        try:
            readiness_diagnostics(args.stage, args.mode)
        except (OSError, RuntimeError, ValueError):
            pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    trial_parser = sub.add_parser("trial")
    trial_parser.add_argument("stage", type=Path)
    trial_parser.add_argument("--old-receiver", required=True)
    trial_parser.add_argument("--asset-root", required=True)
    trial_parser.add_argument("--nginx-config", required=True)
    trial_parser.add_argument("--kiosk-launcher", required=True)
    trial_parser.add_argument("--guard-path", action="append", required=True)
    trial_parser.add_argument("--provider-arg", action="append", default=[])
    trial_parser.add_argument("--check-rollback", action="store_true")
    for mode in ("restore", "verify"):
        sub.add_parser(mode).add_argument("stage", type=Path)
    host = sub.add_parser("verdict")
    host.add_argument("evidence", type=Path)
    host.add_argument("--native-verdict", type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    try:
        if args.mode == "verdict":
            print(host_verdict(args.evidence, args.native_verdict))
        else:
            def interrupted(signum, _frame):
                raise RuntimeError("bounded trial interrupted")
            for sig in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT):
                signal.signal(sig, interrupted)
            if args.mode == "trial":
                trial(args.stage, args)
            elif args.mode == "restore":
                restore(args.stage)
            else:
                verify(args.stage)
    except RuntimeError as error:
        failure_diagnostics(args)
        # Gate messages are fixed strings, never command output or RPC payloads.
        print("STOP: " + str(error) + "; no native-input pass claimed", file=sys.stderr)
        return 1
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired, struct.error, ET.ParseError):
        failure_diagnostics(args)
        # Suppress traceback/command arguments: private unit/policy evidence stays local.
        print("STOP: required trial safety/evidence gate failed; details remain private", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
