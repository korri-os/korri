"""Drive explicit publisher trust and approval choices through the real CLI."""
import errno
import json
import os
import pty
import select
import signal
import sys
import time

executable, *arguments = sys.argv[1:]
answers = json.loads(os.environ["KORRI_PLUGIN_ANSWERS"])
pid, terminal = pty.fork()
if pid == 0:
    os.execv(executable, [executable, *arguments])
transcript = bytearray()
seen = 0
scan_from = 0
deadline = time.monotonic() + 30
try:
    while time.monotonic() < deadline:
        if not select.select([terminal], [], [], 0.1)[0]:
            continue
        try:
            data = os.read(terminal, 65536)
        except OSError as error:
            if error.errno == errno.EIO:
                break
            raise
        if not data:
            break
        transcript.extend(data)
        if seen < len(answers):
            marker, answer = answers[seen]
            if marker.encode() in transcript[scan_from:]:
                scan_from = len(transcript)
                os.write(terminal, answer.encode() + b"\n")
                seen += 1
    else:
        os.kill(pid, signal.SIGKILL)
        raise RuntimeError("CLI did not finish its explicit plugin choices")
finally:
    os.close(terminal)
    _, status = os.waitpid(pid, 0)
sys.stdout.buffer.write(transcript)
assert seen == len(answers), "CLI did not ask for the expected explicit choices"
sys.exit(os.waitstatus_to_exitcode(status))
