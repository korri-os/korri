#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3
"""Drive the real CLI through a PTY; the Rust test owns its typed Unix server."""
import errno
import os
import pty
import select
import signal
import sys
import time

choice, executable, *arguments = sys.argv[1:]
pid, terminal = pty.fork()
if pid == 0:
    os.execv(executable, [executable, *arguments])

transcript = bytearray()
answered = False
deadline = time.monotonic() + 15
try:
    while time.monotonic() < deadline:
        readable, _, _ = select.select([terminal], [], [], 0.1)
        if not readable:
            continue
        try:
            chunk = os.read(terminal, 65536)
        except OSError as error:
            if error.errno == errno.EIO:
                break
            raise
        if not chunk:
            break
        transcript.extend(chunk)
        if not answered and b"Choose game" in transcript:
            os.write(terminal, choice.encode() + b"\n")
            answered = True
    else:
        os.kill(pid, signal.SIGKILL)
        raise RuntimeError("CLI did not finish its terminal selection")
finally:
    os.close(terminal)
    _, status = os.waitpid(pid, 0)
sys.stdout.buffer.write(transcript)
if not answered:
    raise RuntimeError("CLI did not ask for an explicit game selection")
sys.exit(os.waitstatus_to_exitcode(status))
