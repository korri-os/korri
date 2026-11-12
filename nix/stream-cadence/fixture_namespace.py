"""Private X :0 for a same-device pair, without changing Moonlight's binary.

Linux puts abstract Unix sockets in the network namespace, path sockets in
its mount namespace. Both must be private: DISPLAY alone cannot redirect the
product decoder's hardcoded :0. Only root agents can create/join this pair.
"""
import ctypes
import fcntl
import os
from pathlib import Path
import socket
import struct

CLONE_NEWNS = 0x00020000
CLONE_NEWNET = 0x40000000
MS_REC = 16384
MS_PRIVATE = 1 << 18
MS_NOSUID = 2
MS_NODEV = 4
MS_NOEXEC = 8


def mount(source: bytes | None, target: bytes, kind: bytes | None, flags: int, data: bytes | None = None) -> None:
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.mount(source, target, kind, ctypes.c_ulong(flags), data) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), os.fsdecode(target))


def hide_input() -> None:
    """Child-only mount: even its primary group cannot open real input nodes."""
    if os.geteuid() != 0:
        raise RuntimeError('The private product fixture needs root to hide real input devices.')
    os.unshare(CLONE_NEWNS)
    mount(None, b'/', None, MS_PRIVATE | MS_REC)
    if Path('/dev/input').exists():
        mount(b'tmpfs', b'/dev/input', b'tmpfs', MS_NOSUID | MS_NODEV | MS_NOEXEC, b'mode=0555,size=4k')


class PairNamespace:
    def __init__(self) -> None:
        self.original: dict[str, int] = {}
        self.active = False

    def remember(self) -> None:
        if self.original or self.active:
            raise RuntimeError('This agent already has a pair namespace.')
        if os.geteuid() != 0:
            raise RuntimeError('A private PC product pair needs root agents for its X :0 namespaces.')
        self.original = {name: os.open(f'/proc/self/ns/{name}', os.O_RDONLY | os.O_CLOEXEC) for name in ('net', 'mnt')}

    def create(self) -> dict:
        self.remember()
        try:
            os.unshare(CLONE_NEWNET | CLONE_NEWNS)
            self.active = True
            mount(None, b'/', None, MS_PRIVATE | MS_REC)
            # Xwayland also reserves /tmp/.X0-lock. Hiding only the socket
            # directory would leave the product's live lock and choose :1.
            # All run directories are created after this mount and shared by
            # the pair; no file in the product's /tmp is changed.
            mount(b'tmpfs', b'/tmp', b'tmpfs', MS_NOSUID | MS_NODEV, b'mode=1777')
            Path('/tmp/.X11-unix').mkdir(mode=0o1777)
            Path('/tmp/.X11-unix').chmod(0o1777)
            # Bring up only this namespace's loopback. No veth, external address,
            # route, or connection to the product network is made.
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as control:
                request = struct.pack('16sH22x', b'lo', 0)
                flags = struct.unpack('16sH22x', fcntl.ioctl(control, 0x8913, request))[1]
                fcntl.ioctl(control, 0x8914, struct.pack('16sH22x', b'lo', flags | 1))
            return self.record()
        except BaseException:
            self.close()
            raise

    def join(self, record: dict) -> None:
        self.remember()
        try:
            pid = record['pid']
            if not isinstance(pid, int) or pid <= 0:
                raise ValueError('The pair namespace needs a positive agent PID.')
            # Open both before entering either; pin them and check their identity.
            descriptors = {}
            try:
                for name in ('net', 'mnt'):
                    path = f'/proc/{pid}/ns/{name}'
                    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC)
                    descriptors[name] = fd
                    if os.readlink(f'/proc/self/fd/{fd}') != record[name]:
                        raise RuntimeError('The pair namespace identity changed before joining.')
                    if os.fstat(fd).st_ino == os.fstat(self.original[name]).st_ino:
                        raise RuntimeError('The proposed pair namespace is the product namespace.')
                self.active = True
                os.setns(descriptors['net'], CLONE_NEWNET)
                os.setns(descriptors['mnt'], CLONE_NEWNS)
            finally:
                for fd in descriptors.values():
                    os.close(fd)
        except BaseException:
            self.close()
            raise

    def record(self) -> dict:
        return {'pid': os.getpid(), **{name: os.readlink(f'/proc/self/ns/{name}') for name in ('net', 'mnt')}}

    def close(self) -> None:
        # An agent serves exactly one run and exits after close. Stay in its
        # namespace through cleanup: its guard thread may still share fs_struct,
        # so setns(mnt) here would be unsafe. Kernel teardown follows agent exit.
        for fd in self.original.values():
            os.close(fd)
        self.original = {}


def fd_links(pid: int, proc: Path = Path('/proc')) -> set[str] | None:
    try:
        entries = list((proc / str(pid) / 'fd').iterdir())
    except OSError:
        return None
    links = set()
    for entry in entries:
        try:
            links.add(os.readlink(entry))
        except FileNotFoundError:
            continue  # A descriptor closed during the read.
        except OSError:
            return None
    return links


def input_nodes_open(pid: int, proc: Path = Path('/proc')) -> list[str] | None:
    links = fd_links(pid, proc)
    return sorted(link for link in links if link.startswith('/dev/input/')) if links is not None else None


def socket_inodes(links: set[str]) -> set[int]:
    return {int(link[8:-1]) for link in links if link.startswith('socket:[') and link.endswith(']')}


def unix_peers() -> dict[int, dict]:
    """Linux sock_diag supplies peer inodes; /proc/net/unix alone cannot prove the peer."""
    # linux/unix_diag.h: NAME and PEER. Read only this network namespace.
    request = struct.pack('=BBHIIIII', socket.AF_UNIX, 0, 0, 0xffffffff, 0, 1 | 4, 0xffffffff, 0xffffffff)
    header = struct.pack('=IHHII', 16 + len(request), 20, 1 | 0x300, 1, 0)
    rows: dict[int, dict] = {}
    with socket.socket(socket.AF_NETLINK, socket.SOCK_RAW, 4) as control:
        control.settimeout(2)
        control.send(header + request)
        while True:
            packet = control.recv(1 << 20)
            offset = 0
            while offset + 16 <= len(packet):
                length, kind, flags, sequence, pid = struct.unpack_from('=IHHII', packet, offset)
                if length < 16 or offset + length > len(packet) or sequence != 1:
                    raise RuntimeError('Malformed Unix socket diagnostic reply.')
                message = packet[offset + 16:offset + length]
                offset += (length + 3) & ~3
                if flags & 0x10:  # NLM_F_DUMP_INTR: incomplete evidence is no proof.
                    raise RuntimeError('Unix socket diagnostic dump was interrupted.')
                if kind == 3:
                    return rows
                if kind == 2:
                    raise RuntimeError('Unix socket diagnostic request failed.')
                if kind != 20 or len(message) < 16:
                    raise RuntimeError('Unknown Unix socket diagnostic reply.')
                inode = struct.unpack_from('=I', message, 4)[0]
                row: dict = {'peer': None, 'name': ''}
                cursor = 16
                while cursor + 4 <= len(message):
                    size, attribute = struct.unpack_from('=HH', message, cursor)
                    if size < 4 or cursor + size > len(message):
                        raise RuntimeError('Malformed Unix socket diagnostic attribute.')
                    value = message[cursor + 4:cursor + size]
                    if attribute == 0:
                        row['name'] = value.rstrip(b'\0').replace(b'\0', b'@', 1).decode(errors='replace')
                    elif attribute == 2 and len(value) == 4:
                        row['peer'] = struct.unpack('=I', value)[0]
                    cursor += (size + 3) & ~3
                rows[inode] = row


def x_connection(viewer: set[int], xwayland: set[int], peers: dict[int, dict]) -> dict:
    connections: list[dict] = []
    for inode in sorted(viewer):
        peer = peers.get(inode, {}).get('peer')
        name = peers.get(peer, {}).get('name', '') if isinstance(peer, int) else ''
        if name.lstrip('@').startswith('/tmp/.X11-unix/X'):
            connections.append({'viewer_socket': inode, 'peer_socket': peer, 'name': name, 'fixture': peer in xwayland})
    return {
        'connected': bool(connections),
        'pass': bool(connections) and all(row['fixture'] and row['name'].lstrip('@') == '/tmp/.X11-unix/X0' for row in connections),
        'connections': connections,
    }


def isolation(viewer: int, compositor: int) -> dict:
    """Prove the viewer's X peer is our compositor's Xwayland, including daemonized servers."""
    viewer_links = fd_links(viewer)
    if viewer_links is None:
        raise RuntimeError('Cannot read the fixture viewer open files.')
    xwayland = set()
    servers = []
    network = os.readlink('/proc/self/ns/net')
    if os.readlink(f'/proc/{viewer}/ns/net') != network:
        raise RuntimeError('The fixture viewer left the pair network namespace.')
    peers = unix_peers()
    compositor_links = fd_links(compositor)
    if compositor_links is None:
        raise RuntimeError('Cannot read the fixture compositor open files.')
    compositor_sockets = socket_inodes(compositor_links)
    # wlroots double-forks Xwayland, so its PPID can be 1. A process-tree
    # check misses it. Prove both the private network identity and its actual
    # WM/Wayland socket connection to our compositor instead.
    for process in Path('/proc').glob('[0-9]*'):
        child = int(process.name)
        try:
            if os.readlink(process / 'ns/net') != network:
                continue
            if Path(os.readlink(process / 'exe')).name != 'Xwayland':
                continue
            links = fd_links(child)
            if links is None:
                raise RuntimeError('Cannot read the fixture Xwayland open files.')
            sockets = socket_inodes(links)
            if not any(peers.get(inode, {}).get('peer') in compositor_sockets for inode in sockets):
                continue
            servers.append(child)
            xwayland |= sockets
        except (FileNotFoundError, ProcessLookupError):
            continue
    x = x_connection(socket_inodes(viewer_links), xwayland, peers)
    inputs = sorted(link for link in viewer_links if link.startswith('/dev/input/'))
    return {'pass': x['pass'] and not inputs, 'x': x, 'input_nodes': inputs, 'xwayland_pids': servers, 'network': network}
