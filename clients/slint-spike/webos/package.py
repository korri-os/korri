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
import json
import pathlib
import re
import tarfile
import tempfile
import tomllib
import sys

APP_ID = 'org.korri.slint-spike'
ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'nix/webos'))
from ipk import (add_bytes, run, verify_binary, verify_build_record, write_ipk, write_payload)


def app_info(version):
    return {'id': APP_ID, 'version': version, 'vendor': 'Korri',
            'type': 'native', 'main': 'bin/korri-slint-spike',
            'title': 'Korri Slint spike', 'icon': 'icon.png',
            'appDescription': 'Temporary native diagnostics over a real isolated korrid.'}


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
    icon = ROOT / 'brand/icons/icon-192.png'
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
