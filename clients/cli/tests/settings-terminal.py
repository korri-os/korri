#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3
"""Exercise the real CLI with a PTY and check the kernel terminal echo flag."""
import errno
import fcntl
import json
import os
import pty
import select
import signal
import subprocess
import sys
import termios
import time

binary, socket, mode = sys.argv[1:]
master, slave = pty.openpty()
original = termios.tcgetattr(slave)
args = [binary, "metadata", "key", "set", "--socket", socket, "--json"]
if mode == "switch-sigterm":
    args = [binary, "identity", "switch", "local", "--yes", "--socket", socket, "--json"]
# Even explicit private stdin must not read echoed input from a terminal whose
# stdout is redirected. Neither half-terminal case may prompt.
if mode == "stdout-pipe":
    args.append("--key-stdin")
def attach_terminal():
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)


hidden_modes = ("hidden", "sigint", "sigterm", "switch-sigterm")
child = subprocess.Popen(
    args,
    stdin=subprocess.PIPE if mode == "stdin-pipe" else slave,
    stdout=subprocess.PIPE if mode == "stdout-pipe" else slave,
    stderr=subprocess.PIPE if mode not in hidden_modes else slave,
    preexec_fn=attach_terminal,
    env=dict(os.environ, PATH=""),  # Hidden input must not need stty or a shell.
)
transcript = bytearray()
try:
    if mode not in hidden_modes:
        stdout, stderr = child.communicate(timeout=10)
        assert child.returncode == 2, (stdout, stderr)
        assert stdout in (None, b"")
        assert termios.tcgetattr(slave) == original
        failure = json.loads(stderr)
        assert failure["code"] == "Usage"
        print("partial terminal input refused without prompting")
        sys.exit(0)
    def wait_for_prompt(marker):
        deadline = time.monotonic() + 10
        while marker not in transcript:
            if time.monotonic() > deadline:
                raise AssertionError("CLI prompt did not arrive")
            if select.select([master], [], [], 0.02)[0]:
                transcript.extend(os.read(master, 4096))
            assert child.poll() is None, transcript.decode(errors="replace")

    if mode == "switch-sigterm":
        # Reuse the native guard for consecutive secrets in the same process.
        wait_for_prompt(b"Existing person data: transfer or delete")
        os.write(master, b"transfer\n")
        wait_for_prompt(b"Type confirm to acknowledge")
        os.write(master, b"confirm\n")
        wait_for_prompt(b"Encrypted person-key backup:")
        assert not termios.tcgetattr(slave)[3] & termios.ECHO
        os.write(master, b"private-encrypted\n")
        wait_for_prompt(b"Backup password:")

    deadline = time.monotonic() + 10
    while termios.tcgetattr(slave)[3] & termios.ECHO:
        if time.monotonic() > deadline:
            raise AssertionError("CLI did not disable terminal echo")
        if select.select([master], [], [], 0.02)[0]:
            transcript.extend(os.read(master, 4096))
        assert child.poll() is None, transcript.decode(errors="replace")
    with open(f"/proc/{child.pid}/cmdline", "rb") as command_line:
        assert b"private-token" not in command_line.read()
    if mode == "hidden":
        os.write(master, b"private-token\n")
    else:
        # Leave a partial password in the canonical input buffer. Cancellation
        # must restore the terminal without echoing or submitting this input.
        os.write(master, b"private-token")
        if mode == "sigint":
            os.write(master, b"\x03")  # Actual foreground-terminal Ctrl-C.
        else:
            child.send_signal(signal.SIGTERM)
    child.wait(timeout=10)
    assert termios.tcgetattr(slave) == original, "terminal state was not restored"
    if mode != "hidden":
        # A partial password must not become the next command's terminal input.
        os.write(master, b"\n")
        assert select.select([slave], [], [], 1)[0], "terminal input is unavailable"
        assert os.read(slave, 4096) == b"\n", "private input survived cancellation"
    os.close(slave)
    slave = None
    while select.select([master], [], [], 0.1)[0]:
        try:
            data = os.read(master, 4096)
        except OSError as error:
            if error.errno == errno.EIO:
                break
            raise
        if not data:
            break
        transcript.extend(data)
    assert b"private-token" not in transcript, "private input was echoed"
    assert b"private-encrypted" not in transcript, "a previous private input was echoed"
    if mode != "hidden":
        assert child.returncode == 1, transcript.decode(errors="replace")
        failure_start = transcript.index(b'{"code"')
        failure = json.loads(transcript[failure_start:].decode())
        assert failure["code"] == "Cancelled"
        print(f"{mode} cancellation restored terminal without submitting a secret")
        sys.exit(0)
    assert child.returncode == 0, transcript.decode(errors="replace")
    # Prompts use stderr; the result is still exactly the existing response.
    assert b'SteamGridDB key:' in transcript
    result_start = transcript.index(b'{"_tag"')
    result = json.loads(transcript[result_start:].decode())
    assert result["outcome"]["_tag"] == "Ok"
    print("hidden terminal input and restoration verified")
finally:
    if child.poll() is None:
        child.kill()
        child.wait()
    if slave is not None:
        os.close(slave)
    os.close(master)
