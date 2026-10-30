#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#binutils --command python3
"""Package a prebuilt, pinned ARM spike using the existing native webOS format.

Format grounding: Moonlight TV v1.6.36 appinfo.json, packageinfo.json and
control from the inspected IPK, source 89221f66b79e06178a6465503cc5a306d2794786.
The org.korri prefix exists in clients/android/signer-test/build.gradle.
No permissions, services, startup hooks or private RPC bootstrap are packaged.
"""
import argparse
import hashlib
import io
import json
import pathlib
import re
import struct
import subprocess
import tarfile
import tempfile
import tomllib

APP_ID = 'org.korri.slint-spike'
ROOT = pathlib.Path(__file__).resolve().parents[3]


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
    if '[Requesting program interpreter: /lib/ld-linux.so.3]' not in program:
        raise ValueError('expected the TV loader /lib/ld-linux.so.3')
    versions = run('readelf', '--version-info', '-W', str(binary))
    for version in set(re.findall(r'Name: (GLIBC_[A-Za-z0-9_.]+)', versions)):
        match = re.fullmatch(r'GLIBC_(\d+)\.(\d+)(?:\.(\d+))?', version)
        if not match:
            raise ValueError('unrecognized libc requirement: ' + version)
        required = tuple(int(part or 0) for part in match.groups())
        if required > (2, 19, 0):
            raise ValueError('TV glibc 2.19 cannot satisfy ' + version)
    dynamic = run('readelf', '-dW', str(binary))
    if '/nix/store/' in dynamic:
        raise ValueError('target ELF contains a build-host Nix store reference')
    return dynamic


def verify_build_record(binary, directory, expected_source):
    # The fixed cross-build action emits these only after Cargo succeeds and
    # after verifying that the clean source SHA did not change during the build.
    if (directory / 'SOURCE-SHA.txt').read_text().strip() != expected_source:
        raise ValueError('build record does not match the tested source SHA')
    record = (directory / 'korri-slint-spike.sha256').read_text().strip()
    digest, separator, filename = record.partition('  ')
    if not separator or not re.fullmatch(r'[0-9a-f]{64}', digest):
        raise ValueError('invalid build checksum record')
    if pathlib.Path(filename).resolve(strict=True) != binary.resolve(strict=True):
        raise ValueError('build checksum record names a different binary')
    if hashlib.sha256(binary.read_bytes()).hexdigest() != digest:
        raise ValueError('binary changed after its recorded build')
    return digest


def app_info(version):
    return {'id': APP_ID, 'version': version, 'vendor': 'Korri',
            'type': 'native', 'main': 'bin/korri-slint-spike',
            'title': 'Korri Slint spike', 'icon': 'icon.png',
            'appDescription': 'Temporary native diagnostics over a real isolated korrid.'}


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=pathlib.Path)
    parser.add_argument('output', type=pathlib.Path)
    parser.add_argument('--source-sha', required=True)
    parser.add_argument('--build-record', required=True, type=pathlib.Path,
                        help='release directory containing the cross-build producer checksum and source SHA')
    args = parser.parse_args()
    if not re.fullmatch(r'[0-9a-f]{40}', args.source_sha):
        parser.error('--source-sha must be the full tested source SHA')
    if run('git', '-C', str(ROOT), 'rev-parse', 'HEAD').strip() != args.source_sha:
        parser.error('source checkout does not match --source-sha')
    if run('git', '-C', str(ROOT), 'status', '--porcelain').strip():
        parser.error('source checkout must be clean before packaging')
    binary = args.binary.resolve(strict=True)
    verify_build_record(binary, args.build_record.resolve(strict=True), args.source_sha)
    dynamic = verify_binary(binary)
    with (ROOT / 'clients/slint-spike/Cargo.toml').open('rb') as source:
        version = tomllib.load(source)['package']['version']
    target = args.output.resolve()
    target.mkdir(parents=True, exist_ok=True)
    ipk = target / (APP_ID + '_' + version + '_arm.ipk')
    if ipk.exists():
        parser.error('output already exists; choose a fresh output directory')
    icon = ROOT / 'clients/portal/public/icon-192.png'
    appinfo = app_info(version)
    packageinfo = {'id': APP_ID, 'version': version, 'app': APP_ID}
    app_base = 'usr/palm/applications/' + APP_ID + '/'
    package_base = 'usr/palm/packages/' + APP_ID + '/'
    files = [
        (app_base + 'appinfo.json', json.dumps(appinfo).encode(), 0o644),
        (app_base + 'icon.png', icon.read_bytes(), 0o644),
        (app_base + 'bin/korri-slint-spike', binary.read_bytes(), 0o755),
        (package_base + 'packageinfo.json', json.dumps(packageinfo).encode(), 0o644),
    ]
    with tempfile.TemporaryDirectory(prefix='korri-webos-package-') as directory:
        work = pathlib.Path(directory)
        (work / 'debian-binary').write_bytes(b'2.0\n')
        write_payload(work / 'data.tar.gz', files)
        installed = sum(len(content) for _, content, _ in files)
        control = ('Package: ' + APP_ID + '\nVersion: ' + version +
                   '\nSection: misc\nPriority: optional\nArchitecture: arm\n' +
                   'Installed-Size: ' + str(installed) +
                   '\nMaintainer: Korri\nDescription: Native Slint compatibility spike.\n' +
                   'webOS-Package-Format-Version: 2\nwebOS-Packager-Version: x.y.x\n')
        with tarfile.open(work / 'control.tar.gz', 'w:gz', format=tarfile.GNU_FORMAT) as archive:
            add_bytes(archive, 'control', control.encode())
        write_ipk(ipk, work)
    # Provenance stays beside the package, outside the application's runtime data.
    digest = hashlib.sha256(ipk.read_bytes()).hexdigest()
    (target / (ipk.name + '.sha256')).write_text(digest + '  ' + ipk.name + '\n')
    (target / 'SOURCE-SHA.txt').write_text(args.source_sha + '\n')
    (target / 'appinfo.json').write_text(json.dumps(appinfo, indent=2) + '\n')
    (target / 'ELF-DEPENDENCIES.txt').write_text(dynamic)
    print(str(ipk))
    print('SHA256=' + digest)


if __name__ == '__main__':
    main()
