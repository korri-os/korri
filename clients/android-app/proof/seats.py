# SPDX-License-Identifier: GPL-3.0-or-later
"""The holder's input-seat receiver for the play proof (#434 D10), as a fixture.

A holder's korrid launches a game only with its input seats reconciled
(services/korrid/src/host/input_seat.rs): it speaks to the root seat
receiver of `contracts/input` at `KORRID_INPUT_SEAT_CONTROL_SOCKET`. On aka
that receiver is the product's (`/run/korri-input-seat`), which a fixture
must not touch, and the federation fixture makes no input. So this receiver
answers the treaty for the holder alone, and makes no device:

- the coordinator connection: each packet is `COORDINATION_VERSION` (2) and
  JSON (`SeatRequest`); each answer is the whole state (`SeatReply`): the
  count, the session, no occupied seat, no remote source;
- a lease connection: version-1 binary START, STOP and RESET of one launch,
  each answered as success.

It runs as root, as korrid requires of its peer, and only the holder's group
may connect. Every request goes to its log, for the record.
"""

import json
import os
from pathlib import Path
import socket
import tempfile
import threading

COORDINATION_VERSION = 2
LEASE_VERSION = 1
LEASE_OPERATIONS = {1: "start", 2: "stop", 3: "reset"}
MAX_PACKET = 65_536


class SeatReceiver:
    def __init__(self, parent: Path, gid: int, log: Path):
        self.folder = Path(tempfile.mkdtemp(prefix="korri-d10-seats-", dir=parent))
        self.path = self.folder / "control.sock"
        self.log = log.open("a")
        self.lock = threading.Lock()
        self.count = 1
        self.session: "str | None" = None
        self.requests: list[dict] = []
        self.listener = socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET)
        previous = os.umask(0o117)
        try:
            self.listener.bind(str(self.path))
        finally:
            os.umask(previous)
        os.chown(self.path, -1, gid, follow_symlinks=False)
        self.listener.listen(8)
        os.chmod(self.folder, 0o711)
        self.closed = False
        threading.Thread(target=self.accept, daemon=True).start()

    def environment(self) -> dict[str, str]:
        """What the holder's korrid reads (host/mod.rs)."""
        return {"KORRID_INPUT_SEAT_CONTROL_SOCKET": str(self.path)}

    def note(self, entry: dict) -> None:
        with self.lock:
            self.requests.append(entry)
            self.log.write(json.dumps(entry) + "\n")
            self.log.flush()

    def state(self) -> dict:
        return {
            "occupiedLocalSeats": [],
            "failure": None,
            "count": self.count,
            "session": self.session,
            "recoveryRequired": False,
            "slot": None,
            "remoteSources": [],
            "remoteEvents": [],
        }

    def answer(self, request: dict) -> dict:
        operation = request.get("operation")
        with self.lock:
            if operation == "applyCount":
                self.count = int(request["count"])
            elif operation == "beginSession":
                self.session = str(request["launchId"])
            elif operation == "endSession" and self.session == request.get("launchId"):
                self.session = None
            reply = self.state()
            if operation not in {"hello", "poll", "applyCount", "beginSession", "endSession", "route"}:
                reply["failure"] = "invalid"
            return reply

    def accept(self) -> None:
        while not self.closed:
            try:
                connection, _ = self.listener.accept()
            except OSError:
                return
            threading.Thread(target=self.serve, args=(connection,), daemon=True).start()

    def serve(self, connection: socket.socket) -> None:
        with connection:
            while True:
                try:
                    packet = connection.recv(MAX_PACKET)
                except OSError:
                    return
                if not packet:
                    return
                if packet[0] == COORDINATION_VERSION:
                    request = json.loads(packet[1:])
                    if request.get("operation") != "poll":
                        self.note({"coordinator": request})
                    connection.sendall(bytes([COORDINATION_VERSION]) + json.dumps(self.answer(request)).encode())
                elif packet[0] == LEASE_VERSION and len(packet) == 34:
                    operation = LEASE_OPERATIONS.get(packet[1], "unknown")
                    self.note({"lease": operation, "launchId": packet[2:].decode(errors="replace")})
                    connection.sendall(bytes([LEASE_VERSION, 0 if operation != "unknown" else 1, 0]))
                else:
                    self.note({"refused": packet[:8].hex()})
                    return

    def close(self) -> list[dict]:
        self.closed = True
        self.listener.close()
        self.path.unlink(missing_ok=True)
        try:
            self.folder.rmdir()
        except OSError:
            pass
        self.log.close()
        return self.requests
