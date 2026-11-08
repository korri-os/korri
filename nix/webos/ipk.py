"""Native webOS archive format and ARM ABI checks, promoted from the TV spike.

Format grounding: inspected Moonlight TV v1.6.36 IPK, source
89221f66b79e06178a6465503cc5a306d2794786. Never bundles firmware libraries.
"""
import hashlib
import io
import pathlib
import re
import struct
import subprocess
import tarfile


def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT)


def verify_binary(binary):
    with binary.open('rb') as source:
        header = source.read(52)
    if len(header) != 52 or header[:6] != b'\x7fELF\x01\x01':
        raise ValueError('expected a little-endian ELF32 binary')
    machine = struct.unpack_from('<H', header, 18)[0]
    flags = struct.unpack_from('<I', header, 36)[0]
    if machine != 40 or flags >> 24 != 5 or not flags & 0x200 or flags & 0x400:
        raise ValueError('expected ARM EABI5 soft-float calling ABI')
    program = run('readelf', '-lW', str(binary))
    if ('.so' not in binary.name or 'INTERP' in program) and '[Requesting program interpreter: /lib/ld-linux.so.3]' not in program:
        raise ValueError('expected the TV loader /lib/ld-linux.so.3')
    versions = run('readelf', '--version-info', '-W', str(binary))
    for version in set(re.findall(r'Name: (GLIBC_[A-Za-z0-9_.]+)', versions)):
        match = re.fullmatch(r'GLIBC_(\d+)\.(\d+)(?:\.(\d+))?', version)
        if not match:
            raise ValueError('unrecognized libc requirement: ' + version)
        required = tuple(int(part or 0) for part in match.groups())
        if required > (2, 19, 0):
            raise ValueError('TV glibc 2.19 cannot satisfy ' + version)
    for family, limit in (('GLIBCXX', (3, 4, 19)), ('CXXABI', (1, 3, 7))):
        for version in re.findall(r'Name: ' + family + r'_(\d+(?:\.\d+)+)', versions):
            if tuple(map(int, version.split('.'))) > limit:
                raise ValueError('unsupported TV ' + family + ' requirement: ' + version)
    dynamic = run('readelf', '-dW', str(binary))
    paths = re.findall(r'\((?:RPATH|RUNPATH)\).*?\[(.*?)\]', dynamic)
    allowed = {'korri-webos': '$ORIGIN/../lib', 'libss4s.so': '$ORIGIN'}
    if any(path != allowed.get(binary.name) for path in paths):
        raise ValueError('unexpected target library search path')
    if '/nix/store/' in dynamic:
        raise ValueError('target ELF contains a build-host Nix store reference')
    return dynamic


def verify_build_record(binary, directory, expected_source):
    # The fixed cross-build action emits these only after Cargo succeeds and
    # after verifying that the clean source SHA did not change during the build.
    if (directory / 'SOURCE-SHA.txt').read_text().strip() != expected_source:
        raise ValueError('build record does not match the tested source SHA')
    record = (directory / (binary.name + '.sha256')).read_text().strip()
    digest, separator, filename = record.partition('  ')
    if not separator or not re.fullmatch(r'[0-9a-f]{64}', digest):
        raise ValueError('invalid build checksum record')
    if pathlib.Path(filename).resolve(strict=True) != binary.resolve(strict=True):
        raise ValueError('build checksum record names a different binary')
    if hashlib.sha256(binary.read_bytes()).hexdigest() != digest:
        raise ValueError('binary changed after its recorded build')
    return digest


def add_bytes(archive, name, data, mode=0o644):
    member = tarfile.TarInfo(name)
    member.size = len(data)
    member.mode = mode
    member.uid = member.gid = 0
    member.mtime = 0
    archive.addfile(member, io.BytesIO(data))


def write_payload(destination, files):
    # Both inspected native/web reference producers emit explicit parent dirs.
    # Derive only parents of our existing payload; do not add service paths.
    directories = {str(parent) for name, _, _ in files
                   for parent in pathlib.PurePosixPath(name).parents if str(parent) != '.'}
    with tarfile.open(destination, 'w:gz', format=tarfile.GNU_FORMAT) as archive:
        for name in sorted(directories):
            member = tarfile.TarInfo(name + '/')
            member.type = tarfile.DIRTYPE
            member.mode = 0o755
            member.uid = member.gid = 0
            member.mtime = 0
            archive.addfile(member)
        for name, content, mode in files:
            add_bytes(archive, name, content, mode)


def write_ipk(destination, directory):
    # Moonlight and accepted Lightkeeper use System V headers without GNU's
    # trailing name slash, and include the regular-file bits in the mode.
    # GNU ar's normalized listing hides these differences from old extractors.
    with destination.open('xb') as output:
        output.write(b'!<arch>\n')
        for name in ('debian-binary', 'control.tar.gz', 'data.tar.gz'):
            content = (directory / name).read_bytes()
            header = (name.encode('ascii').ljust(16) + b'0'.ljust(12) +
                      b'0'.ljust(6) + b'0'.ljust(6) + b'100644'.ljust(8) +
                      str(len(content)).encode('ascii').ljust(10) + b'`\n')
            if len(header) != 60:
                raise ValueError('archive member exceeds the native header bounds')
            output.write(header)
            output.write(content)
            if len(content) % 2:
                output.write(b'\n')


def native_headers(filename):
    data = pathlib.Path(filename).read_bytes()
    assert data[:8] == b'!<arch>\n', 'archive magic differs'
    offset = 8
    for name in ('debian-binary', 'control.tar.gz', 'data.tar.gz'):
        header = data[offset:offset + 60]
        assert len(header) == 60, 'truncated header'
        assert header[:16] == name.encode().ljust(16), 'native member name differs'
        assert header[40:48].strip() == b'100644', 'native regular-file mode differs'
        assert header[58:60] == b'`\n', 'header terminator differs'
        length = int(header[48:58])
        offset += 60 + length
        if length % 2:
            assert data[offset:offset + 1] == b'\n', 'odd-size member padding differs'
            offset += 1
    assert offset == len(data), 'unexpected trailing archive content'
