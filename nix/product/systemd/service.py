"""VM fixture: record genuine SIGTERM delivery, optionally feed a watchdog."""
import os
import signal
import socket
import sys
import threading
from pathlib import Path

name = sys.argv[1]
marker = Path('/var/lib/freezer-test') / name
marker.parent.mkdir(exist_ok=True)

def notify(message):
    address = os.environ.get('NOTIFY_SOCKET')
    if address:
        with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as sock:
            sock.sendto(message.encode(), address.replace('@', '\0', 1))

def terminate(signum, frame):
    with marker.open('a') as stream:
        stream.write(f'{os.getpid()} SIGTERM\n')
        stream.flush()
        os.fsync(stream.fileno())
    sys.exit(0)

signal.signal(signal.SIGTERM, terminate)
notify('READY=1')
if name == 'inflight':
    with open('/run/freezer-fuse/hold', 'rb', buffering=0) as stream:
        stream.read(1)
if 'WATCHDOG_USEC' in os.environ:
    while True:
        notify('WATCHDOG=1')
        threading.Event().wait(int(os.environ['WATCHDOG_USEC']) / 3_000_000)
else:
    signal.pause()
