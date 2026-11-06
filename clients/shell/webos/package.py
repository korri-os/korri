#!/usr/bin/env python3
"""Build the M1 native shell IPK from pinned binaries and verified surfaces.

App metadata and archive format follow the already accepted Slint spike.
The only credential is created at launch, never included in this package.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'nix/webos'))
from ipk import add_bytes, run, verify_binary, verify_build_record, write_ipk, write_payload

APP_ID = 'org.korri.shell'
APP_BASE = 'usr/palm/applications/' + APP_ID + '/'
PACKAGE_BASE = 'usr/palm/packages/' + APP_ID + '/'


def stage_surfaces(stage, verifier):
    """Use the existing install producer and relative paths for a relocatable IPK.

    Cache.Entry stores relative filenames, sizes and mtimes (verify.rs). Tar
    preserves the normalized zero mtimes. No target verification or new cache
    schema is needed; --installed checks these same stamps after extraction.
    """
    shutil.copytree(ROOT / 'clients/shell/packages', stage / 'packages', symlinks=True)
    for path in (stage / 'packages').rglob('*'):
        if path.is_symlink():
            raise ValueError('surface payload must not contain symlinks')
        os.utime(path, ns=(0, 0))
    result = subprocess.run([str(verifier.resolve(strict=True)), 'install',
                             '--packages', 'packages', '--cache', 'cache'],
                            cwd=stage, text=True, capture_output=True, check=True)
    (stage / 'verification.txt').write_text(result.stdout + result.stderr)
    index = json.loads((stage / 'cache/index.json').read_text())
    entries = index['entries']
    if {entry['id'] for entry in entries} != {'zone', 'pico'} or not all(entry['pass'] for entry in entries):
        raise ValueError('Zone and Pico must pass install-time verification: ' + result.stdout)
    for entry in entries:
        if entry['dir'] != 'packages/' + entry['id']:
            raise ValueError('verification index is not relocatable')
        for relative, size, mtime in entry['files']:
            path = stage / entry['dir'] / relative
            if path.stat().st_size != size or mtime != 0:
                raise ValueError('surface stamp differs from the archive payload')


def app_info(version):
    return {'id': APP_ID, 'version': version, 'vendor': 'Korri',
            'type': 'native', 'main': 'launch.sh', 'title': 'Korri',
            'icon': 'icon.png',
            'appDescription': 'Native Korri shell M1 with local sample data.'}


def payload_files(stage, version):
    files = [(APP_BASE + str(path.relative_to(stage)), path.read_bytes(),
              0o755 if path.name in {'launch.sh', 'korri-shell', 'fixture-korrid'} else 0o644)
             for path in sorted(stage.rglob('*')) if path.is_file()]
    files += [(APP_BASE + 'appinfo.json', json.dumps(app_info(version)).encode(), 0o644),
              (APP_BASE + 'icon.png', (ROOT / 'brand/icons/icon-192.png').read_bytes(), 0o644),
              (PACKAGE_BASE + 'packageinfo.json', json.dumps(
                  {'id': APP_ID, 'version': version, 'app': APP_ID}).encode(), 0o644)]
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=pathlib.Path)
    parser.add_argument('--source-sha', required=True)
    parser.add_argument('--verifier', required=True, type=pathlib.Path)
    args = parser.parse_args()
    if not re.fullmatch(r'[0-9a-f]{40}', args.source_sha):
        parser.error('--source-sha must be a full commit SHA')
    if run('git', '-C', str(ROOT), 'rev-parse', 'HEAD').strip() != args.source_sha:
        parser.error('source checkout does not match --source-sha')
    if run('git', '-C', str(ROOT), 'status', '--porcelain').strip():
        parser.error('source checkout must be clean')
    release = ROOT / '.cache/shell-webos-target/armv7-unknown-linux-gnueabi/release'
    binaries = [release / 'korri-shell', release / 'fixture-korrid']
    dependencies = ''
    for binary in binaries:
        verify_build_record(binary, binary.parent, args.source_sha)
        dependencies += binary.name + '\n' + verify_binary(binary) + '\n'
    with (ROOT / 'clients/shell/Cargo.toml').open('rb') as source:
        version = tomllib.load(source)['package']['version']
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    ipk = output / (APP_ID + '_' + version + '_arm.ipk')
    if ipk.exists():
        parser.error('output already exists; choose a fresh directory')
    with tempfile.TemporaryDirectory(prefix='korri-shell-webos-') as directory:
        work = pathlib.Path(directory)
        stage = work / 'app'
        stage.mkdir()
        stage_surfaces(stage, args.verifier)
        shutil.copyfile(stage / 'verification.txt', output / 'verification.txt')
        (stage / 'verification.txt').unlink()
        (stage / 'bin').mkdir()
        for binary in binaries:
            shutil.copyfile(binary, stage / 'bin' / binary.name)
        shutil.copyfile(ROOT / 'clients/shell/webos/launch.sh', stage / 'launch.sh')
        files = payload_files(stage, version)
        (work / 'debian-binary').write_bytes(b'2.0\n')
        write_payload(work / 'data.tar.gz', files)
        control = ('Package: ' + APP_ID + '\nVersion: ' + version +
                   '\nSection: misc\nPriority: optional\nArchitecture: arm\n' +
                   'Installed-Size: ' + str(sum(len(content) for _, content, _ in files)) +
                   '\nMaintainer: Korri\nDescription: Native Korri shell M1 sample data.\n' +
                   'webOS-Package-Format-Version: 2\nwebOS-Packager-Version: x.y.x\n')
        with tarfile.open(work / 'control.tar.gz', 'w:gz', format=tarfile.GNU_FORMAT) as archive:
            add_bytes(archive, 'control', control.encode())
        if run('git', '-C', str(ROOT), 'rev-parse', 'HEAD').strip() != args.source_sha or run(
                'git', '-C', str(ROOT), 'status', '--porcelain').strip():
            raise ValueError('source changed during packaging')
        write_ipk(ipk, work)
    digest = hashlib.sha256(ipk.read_bytes()).hexdigest()
    (output / (ipk.name + '.sha256')).write_text(digest + '  ' + ipk.name + '\n')
    (output / 'SOURCE-SHA.txt').write_text(args.source_sha + '\n')
    (output / 'ELF-DEPENDENCIES.txt').write_text(dependencies)
    print(str(ipk))
    print('SHA256=' + digest)
    print('SIZE_BYTES=' + str(ipk.stat().st_size))


if __name__ == '__main__':
    main()
