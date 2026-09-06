"""Public CLI checks. Run as root ONLY inside the isolated NixOS test VM."""

import fcntl
import os
import pathlib
import shutil
import subprocess
import sys
import time
import unittest

# Refuse accidental invocation outside the dedicated VM before any filesystem
# mutation. This marker is installed only by offline-selector.nix's VM node.
marker = pathlib.Path("/etc/offline-selector-test-vm")
if (
    os.geteuid() != 0
    or not marker.is_file()
    or marker.read_text() != "isolated-selector-fixture-only\n"
):
    raise SystemExit("refusing selector tests outside the dedicated root test VM")

BINARY, FIRST, SECOND, INVALID = sys.argv[1:]
ROOT = pathlib.Path("/nix/var/nix/gcroots/korri-bundle")
ACK = "--acknowledge-exclusive-quiescence"
COMMAND = [BINARY, "offline-select", FIRST, SECOND, ACK]
COUNT = 0

# The caller, not the selector, owns the lock. No Korri consumer is installed in
# this VM; the acknowledgement is not a service-state detection mechanism.
operator_lock = open("/tmp/offline-selector-test.lock", "x")
fcntl.flock(operator_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)


def check(condition, detail):
    assert condition, detail


def snapshot(path):
    if not path.is_symlink() and not path.exists():
        return None
    stat = path.lstat()
    # readlink can update atime, including during these assertions. Link count
    # and ctime must not change: replacing a hard-linked active changes both.
    return (
        stat.st_dev,
        stat.st_ino,
        stat.st_uid,
        stat.st_gid,
        stat.st_mode,
        stat.st_nlink,
        stat.st_mtime_ns,
        stat.st_ctime_ns,
        os.readlink(path) if path.is_symlink() else None,
    )


def directory_contract(path):
    stat = path.stat()
    # Staging/cleanup may change directory timestamps, not identity or access.
    return (
        stat.st_dev,
        stat.st_ino,
        stat.st_uid,
        stat.st_gid,
        stat.st_mode,
        stat.st_nlink,
    )


def reset(previous=True):
    if ROOT.is_symlink():
        ROOT.unlink()
    elif ROOT.exists():
        shutil.rmtree(ROOT)
    ROOT.mkdir(mode=0o711, parents=True)
    (ROOT / "active").symlink_to(FIRST)
    if previous:
        (ROOT / "previous").symlink_to(SECOND)


def run(arguments=COMMAND, success=False, expected=None, **options):
    global COUNT
    result = subprocess.run(arguments, capture_output=True, text=True, **options)
    check(
        (result.returncode == 0) == success, (arguments, result.stdout, result.stderr)
    )
    if expected:
        check(expected in result.stderr, (expected, result.stderr))
    COUNT += 1
    return result


def refused(arguments=COMMAND, expected=None, **options):
    before = {p.name: snapshot(p) for p in ROOT.iterdir()}
    directory = directory_contract(ROOT)
    run(arguments, expected=expected, **options)
    check(
        before == {p.name: snapshot(p) for p in ROOT.iterdir()},
        "refusal changed selector directory",
    )
    check(directory_contract(ROOT) == directory, "refusal changed directory contract")


def no_effects(trace):
    check(trace.count("execve(") == 1, "selector spawned an executable")
    check(
        "systemctl" not in trace and "connect(" not in trace,
        "selector attempted service effects",
    )


def refused_before_staging(expected, name):
    filesystem_root = snapshot(pathlib.Path("/"))
    selector_root = snapshot(ROOT)
    trace_file = pathlib.Path(f"/tmp/{name}.trace")
    refused(["strace", "-f", "-o", str(trace_file)] + COMMAND, expected=expected)
    check(snapshot(pathlib.Path("/")) == filesystem_root, "refusal changed / metadata")
    check(snapshot(ROOT) == selector_root, "refusal changed selector root metadata")
    trace = trace_file.read_text()
    no_effects(trace)
    check(
        "symlinkat(" not in trace and "renameat(" not in trace, "staged before refusal"
    )
    check('"previous"' not in trace, "looked up previous")
    return trace


class MetadataRefusals(unittest.TestCase):
    def setUp(self):
        reset()

    def test_writable_filesystem_root(self):
        filesystem_root = pathlib.Path("/")
        original = filesystem_root.stat()
        for mode in [0o775, 0o757, 0o777]:
            with self.subTest(mode=oct(mode)):
                reset()
                try:
                    filesystem_root.chmod(mode)
                    trace = refused_before_staging(
                        "must not allow group or other writes", f"root-mode-{mode:o}"
                    )
                    check(', "nix",' not in trace, "traversed children of unsafe /")
                finally:
                    filesystem_root.chmod(original.st_mode & 0o7777)

    def test_nonroot_owned_filesystem_root(self):
        filesystem_root = pathlib.Path("/")
        original = filesystem_root.stat()
        try:
            os.chown(filesystem_root, 65534, original.st_gid)
            trace = refused_before_staging("owned by root", "root-owner")
            check(', "nix",' not in trace, "traversed children of unsafe /")
        finally:
            os.chown(filesystem_root, original.st_uid, original.st_gid)

    def test_active_hard_linked_as_previous(self):
        (ROOT / "previous").unlink()
        os.link(ROOT / "active", ROOT / "previous", follow_symlinks=False)
        check(snapshot(ROOT / "active") == snapshot(ROOT / "previous"), "not aliased")
        check((ROOT / "active").lstat().st_nlink == 2, "not multiply-linked")
        refused_before_staging("exactly one hard link", "hard-linked-previous")

    def test_active_hard_linked_without_aliasing_previous(self):
        os.link(ROOT / "active", ROOT / "alias", follow_symlinks=False)
        check((ROOT / "active").lstat().st_nlink == 2, "not multiply-linked")
        refused_before_staging("exactly one hard link", "hard-linked-alias")


# Run every regression even on RED; finally blocks restore VM / before reporting.
metadata_result = unittest.TextTestRunner(verbosity=2).run(
    unittest.defaultTestLoader.loadTestsFromTestCase(MetadataRefusals)
)
check(metadata_result.wasSuccessful(), "offline selector metadata refusals failed")

reset()
directory = directory_contract(ROOT)
previous = snapshot(ROOT / "previous")
result = run(success=True)
check(result.stdout == f"active={SECOND}\n", result.stdout)
check(os.readlink(ROOT / "active") == SECOND, "wrong selection")
check(snapshot(ROOT / "previous") == previous, "previous changed")
check(directory_contract(ROOT) == directory, "selection changed directory contract")
check(
    set(p.name for p in ROOT.iterdir()) == {"active", "previous"},
    "temporary link leaked",
)
refused(expected="expected-current")

# Recovery must name both exact bundles; previous is not consulted.
run([BINARY, "offline-select", SECOND, FIRST, ACK], success=True)
check(snapshot(ROOT / "previous") == previous, "recovery changed previous")
reset(previous=False)
run(success=True)
check(not (ROOT / "previous").exists(), "created previous")
reset()
(ROOT / "previous").unlink()
(ROOT / "previous").write_text("not a recovery policy")
previous = snapshot(ROOT / "previous")
run(success=True)
check(snapshot(ROOT / "previous") == previous, "touched unrelated previous entry")

reset()
refused(COMMAND[:-1], expected="usage:")
refused(COMMAND + ["extra"], expected="usage:")
refused(
    [BINARY, "offline-select", FIRST, SECOND, "--assume-stopped"], expected="usage:"
)
refused([BINARY, "offline-select", FIRST, INVALID, ACK])
refused([BINARY, "offline-select", FIRST, FIRST, ACK], expected="different")
refused([BINARY, "offline-select", FIRST, SECOND + "/bin", ACK])
refused([BINARY, "offline-select", FIRST, "relative", ACK])
refused([BINARY, "offline-select", FIRST, SECOND + "/.", ACK])
refused([BINARY, "offline-select", FIRST, SECOND + "/", ACK])
refused(
    [
        BINARY,
        "offline-select",
        FIRST,
        "/nix/store/../store/" + pathlib.Path(SECOND).name,
        ACK,
    ]
)
pathlib.Path("/tmp/selector-alias").symlink_to(SECOND)
refused([BINARY, "offline-select", FIRST, "/tmp/selector-alias", ACK])
refused([BINARY, "offline-select", "/tmp/selector-alias", SECOND, ACK])
refused(
    ["setpriv", "--reuid=65534", "--regid=65534", "--clear-groups"] + COMMAND,
    expected="root is required",
)
os.lchown(ROOT / "active", 65534, 65534)
refused(expected="owned by root")
reset()
os.chown(ROOT, 65534, 65534)
refused(expected="owned by root")
os.chown(ROOT, 0, 0)
ROOT.chmod(0o755)
refused(expected="0711")
ROOT.chmod(0o711)
(ROOT / "active").unlink()
refused()
(ROOT / "active").write_text("not a selector")
refused()
(ROOT / "active").unlink()
(ROOT / "active").symlink_to("/tmp/selector-alias")
refused(expected="expected-current")
reset()
shutil.rmtree(ROOT)
run()
check(not ROOT.exists(), "offline selection initialized the root")

# Reject a symbolic-link root and a symbolic-link ancestor without chmod/write-through.
reset()
ROOT.rename("/tmp/selector-real-root")
ROOT.symlink_to("/tmp/selector-real-root")
refused()
ROOT.unlink()
pathlib.Path("/tmp/selector-real-root").rename(ROOT)
parent = ROOT.parent
parent.rename(parent.with_name("gcroots-real"))
parent.symlink_to(parent.with_name("gcroots-real"))
refused()
parent.unlink()
parent.with_name("gcroots-real").rename(parent)
parent_mode = parent.stat().st_mode & 0o7777
parent.chmod(0o777)
refused(expected="must not allow group or other writes")
parent.chmod(parent_mode)

# Exclusive temporary creation must never remove another writer's entries.
reset()
read_fd, write_fd = os.pipe()
pid = os.fork()
if pid == 0:
    os.close(write_fd)
    os.read(read_fd, 1)
    os.close(read_fd)
    os.execv(BINARY, COMMAND)
os.close(read_fd)
collisions = []
for attempt in range(16):
    path = ROOT / f".active.offline.{pid}.{attempt}"
    path.symlink_to(FIRST)
    collisions.append((path, snapshot(path)))
os.write(write_fd, b"x")
os.close(write_fd)
_, status = os.waitpid(pid, 0)
check(
    os.waitstatus_to_exitcode(status) != 0, "exhausted temporary collisions succeeded"
)
check(os.readlink(ROOT / "active") == FIRST, "collision changed active")
check(
    all(snapshot(path) == before for path, before in collisions),
    "collision entry removed",
)
COUNT += 1

# Syscall fault injection executes the real binary and filesystem, not a service stand-in.
reset()
previous = snapshot(ROOT / "previous")
run(
    [
        "strace",
        "-f",
        "-o",
        "/tmp/rename.trace",
        "-e",
        "inject=renameat:error=EIO:when=1",
    ]
    + COMMAND
)
check(os.readlink(ROOT / "active") == FIRST, "rename failure changed active")
check(snapshot(ROOT / "previous") == previous, "rename failure changed previous")
check(
    set(p.name for p in ROOT.iterdir()) == {"active", "previous"},
    "rename failure leaked temp",
)
run(
    ["strace", "-f", "-o", "/tmp/fsync.trace", "-e", "inject=fsync:error=EIO:when=1"]
    + COMMAND,
    expected="keep consumers stopped",
)
check(
    os.readlink(ROOT / "active") == SECOND, "fsync ambiguity caused automatic rollback"
)
check(snapshot(ROOT / "previous") == previous, "fsync failure changed previous")

# Pause after exclusive staging. A new target OR a new inode with the old target
# must abort before rename. The held original inode prevents inode reuse.
for target in [SECOND, FIRST]:
    reset()
    process = subprocess.Popen(
        [
            "strace",
            "-f",
            "-o",
            "/tmp/race.trace",
            "-e",
            "inject=symlinkat:delay_exit=2000000:when=1",
        ]
        + COMMAND,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    deadline = time.monotonic() + 10
    while not list(ROOT.glob(".active.offline.*")):
        check(process.poll() is None and time.monotonic() < deadline, "no staged link")
        time.sleep(0.01)
    (ROOT / "concurrent").symlink_to(target)
    os.replace(ROOT / "concurrent", ROOT / "active")
    concurrent = snapshot(ROOT / "active")
    stdout, stderr = process.communicate(timeout=10)
    check(process.returncode != 0 and "expected-current" in stderr, (stdout, stderr))
    check(snapshot(ROOT / "active") == concurrent, "overwrote concurrent selection")
    check(not list(ROOT.glob(".active.offline.*")), "comparison failure leaked temp")
    COUNT += 1

# The post-sync reopen is a real check, not merely a success message.
reset()
process = subprocess.Popen(
    [
        "strace",
        "-f",
        "-o",
        "/tmp/reopen.trace",
        "-e",
        "inject=fsync:delay_exit=2000000:when=1",
    ]
    + COMMAND,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
)
deadline = time.monotonic() + 10
while os.readlink(ROOT / "active") != SECOND:
    check(process.poll() is None and time.monotonic() < deadline, "no renamed link")
    time.sleep(0.01)
(ROOT / "concurrent").symlink_to(SECOND)
os.replace(ROOT / "concurrent", ROOT / "active")
concurrent = snapshot(ROOT / "active")
stdout, stderr = process.communicate(timeout=10)
check(process.returncode != 0 and "keep consumers stopped" in stderr, (stdout, stderr))
check(snapshot(ROOT / "active") == concurrent, "post-sync failure rewrote active")
COUNT += 1

reset()
run(["strace", "-f", "-o", "/tmp/success.trace"] + COMMAND, success=True)
trace = pathlib.Path("/tmp/success.trace").read_text()
for trace_file in ["success", "rename", "fsync", "race", "reopen"]:
    effects = pathlib.Path(f"/tmp/{trace_file}.trace").read_text()
    no_effects(effects)
check("renameat(" in trace and "fsync(" in trace, "missing durable rename")
check(
    trace.rfind("readlinkat(") > trace.find("fsync("),
    "selection not reopened after fsync",
)
print(f"offline selector: {COUNT} public CLI checks passed")
