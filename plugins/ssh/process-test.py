#!/usr/bin/env python3
"""Build-machine only: ephemeral loopback sshd, no host configuration writes."""

import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time


def run(args, **kwargs):
    return subprocess.run(
        args, check=True, text=True, capture_output=True, timeout=20, **kwargs
    )


def main(package):
    if os.geteuid() != 0:
        raise SystemExit(
            "Run as root on the build machine (temporary loopback listener only)."
        )
    manifest = json.loads((Path(package) / "manifest.json").read_text())
    files = manifest["files"]
    openssh = Path(manifest["packages"]["openssh"])
    # Do not use an existing authorized_keys file or change any account.
    # StrictModes correctly refuses authorized key paths below world-writable
    # /tmp, even with a private leaf. /run supplies a root-owned ancestor.
    with tempfile.TemporaryDirectory(
        prefix="korri-ssh-process-", dir="/run"
    ) as temporary:
        root = Path(temporary)
        root.chmod(0o755)
        state, runtime = root / "state", root / "runtime"
        for path in [state, runtime]:
            path.mkdir(mode=0o700)
        env = dict(
            os.environ,
            STATE_DIRECTORY=str(state),
            RUNTIME_DIRECTORY=str(runtime),
            NOTIFY_SOCKET=str(runtime / "notify"),
        )
        run([files["prepare"]], env=env)
        key = state / "ssh_host_ed25519_key"
        identity = key.read_bytes()
        assert key.stat().st_mode & 0o777 == 0o600
        run([files["prepare"]], env=env)
        assert key.read_bytes() == identity
        # Existing directory entries are identity, even if dangling or damaged.
        # Preparation must fail without replacing any of them or their targets.
        for case in ["dangling", "symlink", "directory", "fifo", "owner", "readable", "writable", "executable", "public-symlink", "public-only"]:
            invalid = root / case
            invalid.mkdir(mode=0o700)
            private = invalid / key.name
            public = invalid / (key.name + ".pub")
            target = invalid / "target"
            target.write_bytes(identity)
            target.chmod(0o600)
            if case == "dangling":
                private.symlink_to(invalid / "missing")
            elif case == "symlink":
                private.symlink_to(target)
            elif case == "directory":
                private.mkdir()
            elif case == "fifo":
                os.mkfifo(private, 0o600)
            elif case == "public-only":
                public.write_text("retained public identity\n")
            else:
                private.write_bytes(identity)
                private.chmod(0o600)
                if case == "owner":
                    os.chown(private, 65534, 65534)
                elif case == "public-symlink":
                    public.symlink_to(target)
                else:
                    private.chmod({"readable": 0o644, "writable": 0o620, "executable": 0o700}[case])
            entry = public if case == "public-only" else private
            before = entry.lstat()
            failure = subprocess.run([files["prepare"]], env=dict(env, STATE_DIRECTORY=str(invalid)), capture_output=True, timeout=10)
            assert failure.returncode != 0, case
            after = entry.lstat()
            assert (before.st_ino, before.st_mode, before.st_uid) == (after.st_ino, after.st_mode, after.st_uid), case
            assert target.read_bytes() == identity, case
            if case in ["dangling", "symlink"]:
                assert private.is_symlink(), case
        second = root / "second-device"
        second.mkdir(mode=0o700)
        run([files["prepare"]], env=dict(env, STATE_DIRECTORY=str(second)))
        assert (second / key.name).read_bytes() != identity
        for name in ["accepted", "rejected"]:
            run(
                [
                    files["ssh-keygen"],
                    "-q",
                    "-t",
                    "ed25519",
                    "-N",
                    "",
                    "-f",
                    str(root / name),
                ]
            )
        authorized = root / "authorized_keys"
        authorized.write_bytes((root / "accepted.pub").read_bytes())
        authorized.chmod(0o644)
        effective = run([files["sshd"], "-G", "-T", "-f", files["config"]]).stdout
        for setting in [
            "port 2222",
            "authenticationmethods publickey",
            "passwordauthentication no",
            "kbdinteractiveauthentication no",
            "permitemptypasswords no",
            "permitrootlogin prohibit-password",
            "usepam yes",
            "authorizedkeysfile %h/.ssh/authorized_keys /etc/ssh/authorized_keys.d/%u",
        ]:
            assert setting in effective.splitlines(), (setting, effective)
        runtime_effective = run(
            [files["start"], "-G", "-T"], env=env
        ).stdout.splitlines()
        assert [line for line in runtime_effective if line.startswith("hostkey ")] == [
            f"hostkey {key}"
        ]
        assert f"pidfile {runtime}/sshd.pid" in runtime_effective
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        command = [
            files["start"],
            "-p",
            str(port),
            "-o",
            "ListenAddress=127.0.0.1",
            "-o",
            f"AuthorizedKeysFile={authorized}",
        ]
        client = [
            str(openssh / "bin/ssh"),
            "-F",
            "/dev/null",
            "-p",
            str(port),
            "-o",
            "BatchMode=yes",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            f"UserKnownHostsFile={root / 'known_hosts'}",
            "-o",
            "ConnectTimeout=2",
        ]
        public = run([files["ssh-keygen"], "-y", "-f", str(key)]).stdout.strip()
        (root / "known_hosts").write_text(f"[127.0.0.1]:{port} {public}\n")
        for _ in range(2):
            with (
                (root / "daemon.log").open("w+") as log,
                socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as notification,
            ):
                notification.bind(env["NOTIFY_SOCKET"])
                notification.settimeout(10)
                daemon = subprocess.Popen(
                    command, env=env, stdout=log, stderr=log, start_new_session=True
                )
                try:
                    assert b"READY=1" in notification.recv(4096), (
                        "native Type=notify requires sshd readiness"
                    )
                    deadline = time.monotonic() + 10
                    while True:
                        if daemon.poll() is not None:
                            log.seek(0)
                            raise AssertionError(log.read())
                        try:
                            with socket.create_connection(
                                ("127.0.0.1", port), timeout=0.2
                            ):
                                break
                        except OSError:
                            assert time.monotonic() < deadline
                            time.sleep(0.05)
                    conflict = subprocess.run(
                        command, env=env, text=True, capture_output=True, timeout=10
                    )
                    assert (
                        conflict.returncode != 0
                        and "Cannot bind any address" in conflict.stderr
                    ), conflict
                    accepted = client + ["-i", str(root / "accepted"), "root@127.0.0.1"]
                    try:
                        assert run(accepted + ["id -u"]).stdout.strip() == "0"
                        assert (
                            "pty-ready"
                            in run(
                                client
                                + [
                                    "-tt",
                                    "-i",
                                    str(root / "accepted"),
                                    "root@127.0.0.1",
                                    "test -t 0 && echo pty-ready",
                                ]
                            ).stdout
                        )
                    except subprocess.CalledProcessError as error:
                        log.flush()
                        log.seek(0)
                        raise AssertionError(error.stderr + log.read()) from error
                    rejected = subprocess.run(
                        client
                        + ["-i", str(root / "rejected"), "root@127.0.0.1", "true"],
                        text=True,
                        capture_output=True,
                        timeout=10,
                    )
                    assert (
                        rejected.returncode == 255
                        and "Permission denied (publickey)" in rejected.stderr
                    ), rejected
                    no_key = subprocess.run(
                        client
                        + ["-o", "PubkeyAuthentication=no", "root@127.0.0.1", "true"],
                        text=True,
                        capture_output=True,
                        timeout=10,
                    )
                    assert no_key.returncode == 255
                finally:
                    os.killpg(daemon.pid, signal.SIGTERM)
                    daemon.wait(timeout=10)
            (runtime / "notify").unlink()
            assert key.read_bytes() == identity
            with socket.socket() as connection:
                assert connection.connect_ex(("127.0.0.1", port)) != 0
        key.write_text("damaged identity\n")
        failure = subprocess.run(
            [files["prepare"]], env=env, capture_output=True, timeout=10
        )
        assert failure.returncode != 0
        assert key.read_text() == "damaged identity\n"
    print(
        "Verified: native readiness, key-only root login, PTY, wrong/no-key rejection, per-device keys, restart identity, listener stop, occupied-port and damaged-key refusal."
    )


if __name__ == "__main__":
    main(sys.argv[1])
