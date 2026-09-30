"""VM-only FUSE fixture: hold a read in a non-freezable kernel wait."""
import os
import stat
from pathlib import Path
from fuse import FUSE, Operations

class BlockedFile(Operations):
    def init(self, path):
        Path('/run/freezer-fuse-ready').touch()

    def getattr(self, path, fh=None):
        return dict(st_mode=(stat.S_IFDIR | 0o755) if path == '/' else (stat.S_IFREG | 0o444),
                    st_nlink=2 if path == '/' else 1, st_size=1)

    def readdir(self, path, fh):
        return ['.', '..', 'hold']

    def open(self, path, flags):
        return 0

    def read(self, path, size, offset, fh):
        # The test consumes readiness, starts FreezeUnit, observes 'freezing',
        # then submits StopUnit. Only after cancellation may this read finish.
        Path('/run/freezer-read-ready').write_text('ready\n')
        Path('/run/freezer-read-release').read_text()
        return b'x' if offset == 0 else b''

os.makedirs('/run/freezer-fuse', exist_ok=True)
for path in ['/run/freezer-read-ready', '/run/freezer-read-release']:
    os.mkfifo(path)
FUSE(BlockedFile(), '/run/freezer-fuse', foreground=True, nothreads=True)
