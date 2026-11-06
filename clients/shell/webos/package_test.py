#!/usr/bin/env python3
"""Check native M2 archives without contacting or executing anything on a TV."""
import io
import json
import pathlib
import subprocess
import sys
import tarfile
import tempfile
import unittest

sys.dont_write_bytecode = True
from package import APP_BASE, APP_ID, app_info, payload_files
from ipk import native_headers, verify_binary, verify_build_record, write_ipk, write_payload


def check_ipk(path):
    native_headers(path)
    data = subprocess.check_output(['ar', 'p', str(path), 'data.tar.gz'])
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
        names = set(archive.getnames())
        expected = {'launch.sh', 'bin/korri-shell', 'bin/fixture-korrid', 'bin/webos-korrid',
                    'cache/index.json', 'cache/surfaces/zone.json', 'cache/surfaces/pico.json',
                    'packages/zone/main.slint', 'packages/pico/main.slint',
                    'packages/zone/logic.js', 'packages/pico/logic.js'}
        assert {APP_BASE + name for name in expected} <= names
        assert APP_BASE + 'bin/webos-owner-sign' not in names
        assert not any('KORRID_RPC_CAPABILITY' in name or name.endswith('.so') or '.so.' in name for name in names)
        metadata = json.load(archive.extractfile(APP_BASE + 'appinfo.json'))
        assert metadata['id'] == APP_ID and metadata['type'] == 'native'
        assert metadata['main'] == 'launch.sh'
        for name in ('launch.sh', 'bin/korri-shell', 'bin/fixture-korrid', 'bin/webos-korrid'):
            assert archive.getmember(APP_BASE + name).mode == 0o755
        index = json.load(archive.extractfile(APP_BASE + 'cache/index.json'))
        assert {entry['id'] for entry in index['entries']} == {'zone', 'pico'}
        for entry in index['entries']:
            assert entry['pass'] and entry['dir'] == 'packages/' + entry['id']
            for relative, size, mtime in entry['files']:
                member = archive.getmember(APP_BASE + entry['dir'] + '/' + relative)
                assert member.isfile() and member.size == size and member.mtime * 10**9 == mtime
            summary = json.load(archive.extractfile(APP_BASE + 'cache/surfaces/' + entry['id'] + '.json'))
            assert summary['pass']
        with tempfile.TemporaryDirectory() as directory:
            for name in ('korri-shell', 'fixture-korrid', 'webos-korrid'):
                binary = pathlib.Path(directory) / name
                binary.write_bytes(archive.extractfile(APP_BASE + 'bin/' + name).read())
                verify_binary(binary)
    print('M2_IPK_PAYLOAD_ELF_AND_STAMPS_PASS')


class PackageTests(unittest.TestCase):
    def test_native_headers_and_odd_member_padding(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            for name, data in [('debian-binary', b'2.0\n'), ('control.tar.gz', b'odd'), ('data.tar.gz', b'even')]:
                (work / name).write_bytes(data)
            write_ipk(work / 'app.ipk', work)
            native_headers(work / 'app.ipk')
            self.assertEqual(subprocess.check_output(['ar', 'p', str(work / 'app.ipk'), 'control.tar.gz']), b'odd')
            with self.assertRaises(FileExistsError):
                write_ipk(work / 'app.ipk', work)

    def test_payload_normalizes_modes_and_preserves_zero_mtimes(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = pathlib.Path(directory) / 'app'
            (stage / 'bin').mkdir(parents=True)
            (stage / 'bin/korri-shell').write_bytes(b'ELF bytes')
            (stage / 'launch.sh').write_bytes(b'launcher')
            files = payload_files(stage, '0.0.0')
            destination = pathlib.Path(directory) / 'data.tar.gz'
            write_payload(destination, files)
            with tarfile.open(destination) as archive:
                self.assertEqual(archive.getmember(APP_BASE + 'launch.sh').mode, 0o755)
                self.assertEqual(archive.getmember(APP_BASE + 'bin/korri-shell').mode, 0o755)
                self.assertTrue(all(member.mtime == 0 for member in archive.getmembers()))
                self.assertTrue(all(member.uid == member.gid == 0 for member in archive.getmembers()))
                self.assertEqual(json.load(archive.extractfile(APP_BASE + 'appinfo.json')), app_info('0.0.0'))

    def test_build_record_is_bound_to_binary_name_source_and_digest(self):
        import hashlib
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            binary = work / 'fixture-korrid'
            binary.write_bytes(b'original')
            (work / 'SOURCE-SHA.txt').write_text('a' * 40)
            digest = hashlib.sha256(binary.read_bytes()).hexdigest()
            (work / 'fixture-korrid.sha256').write_text(digest + '  ' + str(binary))
            self.assertEqual(verify_build_record(binary, work, 'a' * 40), digest)
            with self.assertRaises(ValueError):
                verify_build_record(binary, work, 'b' * 40)
            binary.write_bytes(b'changed')
            with self.assertRaises(ValueError):
                verify_build_record(binary, work, 'a' * 40)

    def test_host_elf_is_rejected(self):
        with self.assertRaises(ValueError):
            verify_binary(pathlib.Path('/proc/self/exe'))


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--check-ipk':
        check_ipk(pathlib.Path(sys.argv[2]))
    else:
        unittest.main()
