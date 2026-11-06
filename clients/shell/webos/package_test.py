#!/usr/bin/env python3
"""Check native M2 archives without contacting or executing anything on a TV."""
import argparse
import io
import os
import json
import pathlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

sys.dont_write_bytecode = True
from package import APP_BASE, APP_ID, ROOT, app_info, payload_files, stage_product_relays
from ipk import native_headers, verify_binary, verify_build_record, write_ipk, write_payload


def check_ipk(path, product_relays):
    native_headers(path)
    data = subprocess.check_output(['ar', 'p', str(path), 'data.tar.gz'])
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
        names = set(archive.getnames())
        expected = {'product-relays.json', 'launch.sh', 'bin/korri-shell', 'bin/fixture-korrid', 'bin/webos-korrid',
                    'cache/index.json', 'cache/surfaces/zone.json', 'cache/surfaces/pico.json',
                    'packages/zone/main.slint', 'packages/pico/main.slint',
                    'packages/zone/logic.js', 'packages/pico/logic.js'}
        assert {APP_BASE + name for name in expected} <= names
        assert APP_BASE + 'bin/webos-owner-sign' not in names
        assert not any('KORRID_RPC_CAPABILITY' in name or name.endswith('.so') or '.so.' in name for name in names)
        assert archive.extractfile(APP_BASE + 'product-relays.json').read() == product_relays.read_bytes()
        assert archive.getmember(APP_BASE + 'product-relays.json').mode == 0o644
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
    product_relays = None

    def test_product_relays_are_shipped_unchanged_outside_user_data(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = pathlib.Path(directory)
            stage_product_relays(stage, self.product_relays)
            self.assertEqual((stage / 'product-relays.json').read_bytes(),
                             self.product_relays.read_bytes())
            files = {name: (content, mode) for name, content, mode in payload_files(stage, '0.0.0')}
            self.assertEqual(files[APP_BASE + 'product-relays.json'],
                             (self.product_relays.read_bytes(), 0o644))
            self.assertFalse(any('device.yaml' in name for name in files))

    def test_malformed_product_relays_fail_before_staging(self):
        for value in ('{}', '[]', '[1]', '[""]', 'invalid JSON'):
            with self.subTest(value=value), tempfile.TemporaryDirectory() as directory:
                stage = pathlib.Path(directory)
                source = stage / 'source.json'
                source.write_text(value)
                with self.assertRaises(ValueError):
                    stage_product_relays(stage, source)
                self.assertFalse((stage / 'product-relays.json').exists())

    @unittest.skipIf(os.geteuid() == 0, 'the shipped launcher refuses root')
    def test_launcher_relays_are_product_composition_for_real_brain_only(self):
        expected = json.loads(self.product_relays.read_bytes())
        self.assertTrue(expected)
        self.assertTrue(all(isinstance(value, str) for value in expected))
        for fixture in (False, True):
            with self.subTest(fixture=fixture), tempfile.TemporaryDirectory() as directory:
                app = pathlib.Path(directory)
                (app / 'bin').mkdir()
                (app / 'tmp').mkdir()
                shutil.copyfile(ROOT / 'clients/shell/webos/launch.sh', app / 'launch.sh')
                (app / 'launch.sh').chmod(0o755)
                if not fixture:
                    shutil.copyfile(self.product_relays, app / 'product-relays.json')
                brain = app / 'bin' / ('fixture-korrid' if fixture else 'webos-korrid')
                brain.write_text('''#!/usr/bin/env python3
import json, os, pathlib, signal
pathlib.Path('brain-relays.json').write_text(json.dumps(os.environ.get('KORRID_RELAYS')))
print('KORRID_ADDRESS=127.0.0.1:1', flush=True)
print('KORRID_SHELL_ORIGIN=http://127.0.0.1:1', flush=True)
signal.pause()
''')
                brain.chmod(0o755)
                shell = app / 'bin/korri-shell'
                shell.write_text('''#!/usr/bin/env python3
import json, os, pathlib
pathlib.Path('shell-relays.json').write_text(json.dumps(os.environ.get('KORRID_RELAYS')))
''')
                shell.chmod(0o755)
                env = dict(os.environ, TMPDIR=str(app / 'tmp'),
                           KORRID_RELAYS='["wss://not-the-product.example"]')
                result = subprocess.run([str(app / 'launch.sh')] + (['--fixture'] if fixture else []),
                                        env=env, cwd=app, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                actual = json.loads((app / 'brain-relays.json').read_text())
                if fixture:
                    # Explicit M1 fallback must not need or inherit product relays.
                    self.assertIsNone(actual)
                else:
                    self.assertEqual(json.loads(actual), expected)
                self.assertIsNone(json.loads((app / 'shell-relays.json').read_text()))

    @unittest.skipIf(os.geteuid() == 0, 'the shipped launcher refuses root')
    def test_missing_product_relays_never_starts_a_brain_or_fallback(self):
        with tempfile.TemporaryDirectory() as directory:
            app = pathlib.Path(directory)
            (app / 'bin').mkdir()
            (app / 'tmp').mkdir()
            shutil.copyfile(ROOT / 'clients/shell/webos/launch.sh', app / 'launch.sh')
            (app / 'launch.sh').chmod(0o755)
            for name in ('webos-korrid', 'fixture-korrid'):
                binary = app / 'bin' / name
                binary.write_text('#!/bin/sh\ntouch brain-started\n')
                binary.chmod(0o755)
            result = subprocess.run([str(app / 'launch.sh')], cwd=app,
                                    env=dict(os.environ, TMPDIR=str(app / 'tmp')),
                                    capture_output=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((app / 'brain-started').exists())

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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--product-relays', type=pathlib.Path, required=True)
    parser.add_argument('--check-ipk', type=pathlib.Path)
    args, unittest_args = parser.parse_known_args()
    if args.check_ipk:
        check_ipk(args.check_ipk, args.product_relays)
    else:
        PackageTests.product_relays = args.product_relays
        unittest.main(argv=[sys.argv[0], *unittest_args])
