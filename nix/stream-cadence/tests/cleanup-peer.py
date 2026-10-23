"""Run the real agent loop with local discovery and native certificate IPC.

The certificate provider runs as this test's existing UID instead of requiring
privilege changes. The credential-restricted adapter is tested separately.
A profiling hook injects a real OS signal exactly when a cleanup step starts.
"""

import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time

if sys.argv[1] == "source":
    time.sleep(60)
    raise SystemExit(0)

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import device

root = Path(sys.argv[1])
step = sys.argv[2]
signum = int(sys.argv[3])


def discover():
    return {
        "processes": {},
        "certificate_control": {"path": str(root / "control.sock")},
    }


def certificate(control, request):
    with socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET) as stream:
        stream.settimeout(2)
        stream.connect(control["path"])
        stream.sendall(json.dumps(request).encode())
        return json.loads(stream.recv(16384))


device.discover = discover
device.certificate = certificate
session = device.Session()
session.root = root / "run-data"
session.root.mkdir()
session.source_log = (session.root / "motion.log").open("w")
session.source = subprocess.Popen(
    [sys.executable, __file__, "source"],
    cwd=session.root,
    stdin=subprocess.DEVNULL,
    stdout=session.source_log,
    stderr=subprocess.STDOUT,
)
(root / "source-pid").write_text(str(session.source.pid))
session.revoke = {
    "operation": "revoke",
    "hostUuid": "cleanup-test",
    "certificate": "public-certificate",
}


# Preserve the real Session constructor/loop. Only discovery and the privileged
# adapter above are local providers; this factory supplies the active session.
def active_session():
    return session


device.Session = active_session

target = {
    "source": session.stop_source,
    "certificate": certificate,
    "folder": shutil.rmtree,
}[step].__code__


def inject(frame, event, _arg):
    if event == "call" and frame.f_code is target:
        sys.setprofile(None)
        (root / "signalled").write_text(str(signum))
        os.kill(os.getpid(), signum)


sys.setprofile(inject)
device.main()
