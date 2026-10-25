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
label = os.environ.get("KORRI_PICKER_LABEL", "game")
mode = os.environ.get("KORRI_PICKER_MODE", "terminal")
expect_prompt = os.environ.get("KORRI_PICKER_NO_PROMPT") != "1"
input_read, input_write = os.pipe()
output_read, output_write = os.pipe()
pid, terminal = pty.fork()
if pid == 0:
    if mode == "stdin-pipe":
        os.dup2(input_read, 0)
    elif mode == "stdout-pipe":
        os.dup2(output_write, 1)
    for descriptor in (input_read, input_write, output_read, output_write):
        os.close(descriptor)
    os.execv(executable, [executable, *arguments])
os.close(input_read)
os.close(input_write)
os.close(output_write)

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
        if not answered and f"Choose {label}".encode() in transcript:
            if not expect_prompt:
                raise RuntimeError("CLI prompted when explicit input was required")
            os.write(terminal, choice.encode() + b"\n")
            answered = True
    else:
        os.kill(pid, signal.SIGKILL)
        raise RuntimeError("CLI did not finish its terminal selection")
finally:
    os.close(terminal)
    _, status = os.waitpid(pid, 0)
    transcript.extend(os.read(output_read, 65536))
    os.close(output_read)
sys.stdout.buffer.write(transcript)
if expect_prompt and not answered:
    raise RuntimeError(f"CLI did not ask for an explicit {label} selection")
sys.exit(os.waitstatus_to_exitcode(status))
