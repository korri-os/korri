#!/usr/bin/env python3
"""Check native M2 archives without contacting or executing anything on a TV."""
import argparse
import io
import os
import json
import pathlib
import shutil
import signal
import time
import subprocess
import sys
import tarfile
import tempfile
import unittest

sys.dont_write_bytecode = True
from package import APP_BASE, APP_ID, ROOT, app_info, payload_files, stage_product_relays, audit_payload  # noqa: E402 - local tooling modules need the explicit source path
from ipk import native_headers, verify_binary, verify_build_record, write_ipk, write_payload  # noqa: E402 - local tooling modules need the explicit source path


def check_ipk(path, product_relays):
    native_headers(path)
    data = subprocess.check_output(['ar', 'p', str(path), 'data.tar.gz'])
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
        names = set(archive.getnames())
        assert len(names) == len(archive.getmembers()), "duplicate payload names"
        assert all(member.isfile() or member.isdir() for member in archive.getmembers())
        expected = {'product-relays.json', 'bin/korri-webos', 'lib/libss4s.so', 'lib/ss4s-ndl-webos4.so', 'bin/korri-shell', 'bin/fixture-korrid', 'bin/webos-korrid',
                    'cache/index.json', 'cache/surfaces/zone.json', 'cache/surfaces/pico.json',
                    'packages/zone/main.slint', 'packages/pico/main.slint',
                    'packages/zone/logic.js', 'packages/pico/logic.js'}
        assert {APP_BASE + name for name in expected} <= names
        assert APP_BASE + 'bin/webos-owner-sign' not in names
        assert not any('KORRID_RPC_CAPABILITY' in name for name in names)
        assert APP_BASE + 'launch.sh' not in names
        assert {name for name in names if name.endswith('.so') or '.so.' in name} == {APP_BASE + 'lib/libss4s.so', APP_BASE + 'lib/ss4s-ndl-webos4.so'}
        assert archive.extractfile(APP_BASE + 'product-relays.json').read() == product_relays.read_bytes()
        assert archive.getmember(APP_BASE + 'product-relays.json').mode == 0o644
        metadata = json.load(archive.extractfile(APP_BASE + 'appinfo.json'))
        assert metadata['id'] == APP_ID and metadata['type'] == 'native'
        assert metadata['main'] == 'bin/korri-webos'
        for name in ('bin/korri-webos', 'bin/korri-shell', 'bin/fixture-korrid', 'bin/webos-korrid'):
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
            stage = pathlib.Path(directory)
            for member in archive.getmembers():
                if member.isfile() and member.name.startswith(APP_BASE):
                    relative = pathlib.PurePosixPath(member.name).relative_to(APP_BASE)
                    if '..' in relative.parts:
                        raise ValueError('payload path escapes the app directory')
                    target = stage / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(archive.extractfile(member).read())
                    if target.read_bytes()[:4] == b'\x7fELF':
                        verify_binary(target)
            audit_payload(stage)
    print('M2_IPK_PAYLOAD_ELF_AND_STAMPS_PASS')


class PackageTests(unittest.TestCase):
    product_relays = None

    @classmethod
    def setUpClass(cls):
        cls.native = tempfile.TemporaryDirectory()
        cls.launcher = pathlib.Path(cls.native.name) / 'korri-webos'
        subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra', '-Werror', '-O2',
                        str(ROOT / 'clients/shell/webos/launcher.c'), '-o', str(cls.launcher)], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.native.cleanup()

    def stage_launcher(self, app):
        (app / 'bin').mkdir(exist_ok=True)
        (app / 'tmp').mkdir(exist_ok=True)
        shutil.copyfile(self.launcher, app / 'bin/korri-webos')
        (app / 'bin/korri-webos').chmod(0o755)

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
                self.stage_launcher(app)
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
                result = subprocess.run([str(app / 'bin/korri-webos')] + (['--fixture'] if fixture else []),
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
            self.stage_launcher(app)
            for name in ('webos-korrid', 'fixture-korrid'):
                binary = app / 'bin' / name
                binary.write_text('#!/bin/sh\ntouch brain-started\n')
                binary.chmod(0o755)
            result = subprocess.run([str(app / 'bin/korri-webos')], cwd=app,
                                    env=dict(os.environ, TMPDIR=str(app / 'tmp')),
                                    capture_output=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((app / 'brain-started').exists())

    def stage_processes(self, app, shell_exit=0, brain_behavior='ready', shell_behavior='exit'):
        self.stage_launcher(app)
        shutil.copyfile(self.product_relays, app / 'product-relays.json')
        brain = app / 'bin/webos-korrid'
        brain.write_text("#!/usr/bin/env python3\nimport json, os, pathlib, signal, sys\npathlib.Path('brain.pid').write_text(str(os.getpid()))\nif BEHAVIOR == 'fail':\n    sys.exit(19)\nif BEHAVIOR == 'ready':\n    print('KORRID_ADDRESS=127.0.0.1:1234', flush=True)\n    print('UNTRUSTED=$(touch evaluated)', flush=True)\n    print('KORRID_SHELL_ORIGIN=http://127.0.0.1:1234', flush=True)\nsignal.pause()\n".replace('BEHAVIOR', repr(brain_behavior)))
        brain.chmod(0o755)
        shell = app / 'bin/korri-shell'
        shell.write_text("#!/usr/bin/env python3\nimport json, os, pathlib, resource, signal, stat, sys\ncredentials = pathlib.Path(os.environ['CREDENTIALS_DIRECTORY'])\npathlib.Path('observed.json').write_text(json.dumps({\n    'argv': sys.argv[1:], 'env': dict(os.environ),\n    'credentials_mode': stat.S_IMODE(credentials.stat().st_mode),\n    'work_mode': stat.S_IMODE(credentials.parent.stat().st_mode),\n    'umask': os.umask(0o077), 'core': resource.getrlimit(resource.RLIMIT_CORE),\n    'pid': os.getpid()}))\nif BEHAVIOR == 'wait':\n    signal.pause()\nsys.exit(EXIT_CODE)\n".replace('BEHAVIOR', repr(shell_behavior)).replace('EXIT_CODE', str(shell_exit)))
        shell.chmod(0o755)

    def launch(self, app, *args):
        return subprocess.Popen([str(app / 'bin/korri-webos'), *args], cwd='/',
                                env=dict(os.environ, TMPDIR=str(app / 'tmp'),
                                         KORRI_INPUT_TRACE='1', KORRI_SHELL_SEE_THROUGH='1'),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def assert_cleaned(self, app):
        self.assertEqual(list((app / 'tmp').glob('korri-shell.*')), [])
        for name in ('brain.pid',):
            if (app / name).exists():
                with self.assertRaises(ProcessLookupError):
                    os.kill(int((app / name).read_text()), 0)

    def test_launcher_preserves_arguments_environment_permissions_and_shell_status(self):
        for args, order in (([], 'zone,pico'), (['{"ignored":"$(touch evaluated)"}'], 'zone,pico'),
                            (['--surfaces', 'pico,zone'], 'pico,zone')):
            with self.subTest(args=args), tempfile.TemporaryDirectory() as directory:
                app = pathlib.Path(directory)
                self.stage_processes(app, shell_exit=37)
                sibling = app / 'tmp/keep'
                sibling.mkdir()
                (sibling / 'data').write_text('keep')
                with self.launch(app, *args) as process:
                    _, stderr = process.communicate(timeout=10)
                    self.assertEqual(process.returncode, 37, stderr)
                data = json.loads((app / 'observed.json').read_text())
                self.assertEqual(data['argv'], ['--packages', 'packages', '--cache', 'cache',
                                               '--installed', '--surfaces', order, '--placement', 'fullscreen'])
                self.assertEqual(data['credentials_mode'], 0o700)
                self.assertEqual(data['work_mode'], 0o700)
                self.assertEqual(data['umask'], 0o077)
                self.assertEqual(data['core'], [0, 0])
                for key, value in (('APPID', APP_ID), ('SLINT_SCALE_FACTOR', '1.5'),
                                   ('KORRID_ADDRESS', '127.0.0.1:1234'),
                                   ('KORRID_SHELL_ORIGIN', 'http://127.0.0.1:1234'),
                                   ('KORRI_INPUT_TRACE', '1'), ('KORRI_SHELL_SEE_THROUGH', '1')):
                    self.assertEqual(data['env'][key], value)
                self.assertNotIn('UNTRUSTED', data['env'])
                self.assertFalse((app / 'evaluated').exists())
                self.assertEqual((sibling / 'data').read_text(), 'keep')
                self.assert_cleaned(app)
                self.assertIn('korri-shell launch', (app / 'tmp/korri-shell-launch.log').read_text())

    def test_signals_kill_and_reap_both_children_and_remove_only_launch_directory(self):
        for sig in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT):
            with self.subTest(sig=sig), tempfile.TemporaryDirectory() as directory:
                app = pathlib.Path(directory)
                self.stage_processes(app, shell_behavior='wait')
                with self.launch(app) as process:
                    for _ in range(500):
                        if (app / 'observed.json').exists():
                            break
                        time.sleep(0.01)
                    else:
                        process.kill()
                        self.fail('shell did not start')
                    # observed.json is complete before the script sleeps.
                    time.sleep(0.05)
                    shell_pid = json.loads((app / 'observed.json').read_text())['pid']
                    process.send_signal(sig)
                    process.communicate(timeout=10)
                    self.assertEqual(process.returncode, 143)
                with self.assertRaises(ProcessLookupError):
                    os.kill(shell_pid, 0)
                self.assert_cleaned(app)

    def test_cleanup_does_not_follow_symlinks_and_log_appends_across_launches(self):
        with tempfile.TemporaryDirectory() as directory:
            app = pathlib.Path(directory)
            self.stage_processes(app)
            outside = app / 'outside'
            outside.mkdir()
            (outside / 'data').write_text('keep')
            shell = app / 'bin/korri-shell'
            shell.write_text(shell.read_text().replace("sys.exit(0)",
                "(credentials.parent / 'outside').symlink_to(pathlib.Path('outside').resolve())\nsys.exit(0)"))
            for _ in range(2):
                with self.launch(app) as process:
                    process.communicate(timeout=5)
                    self.assertEqual(process.returncode, 0)
                self.assert_cleaned(app)
            self.assertEqual((outside / 'data').read_text(), 'keep')
            self.assertEqual((app / 'tmp/korri-shell-launch.log').read_text().count('korri-shell launch'), 2)

    @unittest.skipUnless(shutil.which('bwrap'), 'bubblewrap is required to test root without host root')
    def test_root_is_rejected_before_creating_launch_files(self):
        result = subprocess.run(['bwrap', '--unshare-user', '--uid', '0', '--gid', '0',
                                 '--ro-bind', '/', '/', '--proc', '/proc', '--dev', '/dev',
                                 str(self.launcher)], capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('launcher refuses root', result.stderr)

    def test_brain_failure_and_timeout_never_start_shell(self):
        for behavior in ('fail', 'timeout'):
            with self.subTest(behavior=behavior), tempfile.TemporaryDirectory() as directory:
                app = pathlib.Path(directory)
                self.stage_processes(app, brain_behavior=behavior)
                with self.launch(app) as process:
                    process.communicate(timeout=15)
                    self.assertEqual(process.returncode, 1)
                self.assertFalse((app / 'observed.json').exists())
                self.assert_cleaned(app)
                self.assertIn('korrid exited' if behavior == 'fail' else 'startup timed out',
                              (app / 'tmp/korri-shell-launch.log').read_text())

    def test_signal_received_between_fork_and_exec_does_not_start_a_child(self):
        with tempfile.TemporaryDirectory() as directory:
            app = pathlib.Path(directory)
            self.stage_processes(app)
            source = app / 'signal-before-exec.c'
            source.write_text('#define _GNU_SOURCE\n#include <dlfcn.h>\n#include <signal.h>\n#include <unistd.h>\npid_t fork(void) {\n    pid_t (*real_fork)(void) = dlsym(RTLD_NEXT, "fork");\n    pid_t pid = real_fork();\n    if (pid == 0)\n        raise(SIGTERM);\n    return pid;\n}\n')
            library = app / 'signal-before-exec.so'
            subprocess.run(['cc', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror',
                            str(source), '-ldl', '-o', str(library)], check=True)
            result = subprocess.run([str(app / 'bin/korri-webos')], cwd='/',
                                    env=dict(os.environ, TMPDIR=str(app / 'tmp'),
                                             LD_PRELOAD=str(library)), capture_output=True, timeout=5)
            self.assertEqual(result.returncode, 1)
            self.assertFalse((app / 'brain.pid').exists())
            self.assert_cleaned(app)

    def test_signal_during_brain_startup_reaps_brain(self):
        with tempfile.TemporaryDirectory() as directory:
            app = pathlib.Path(directory)
            self.stage_processes(app, brain_behavior='timeout')
            with self.launch(app) as process:
                for _ in range(500):
                    if (app / 'brain.pid').exists():
                        break
                    time.sleep(0.01)
                process.terminate()
                process.communicate(timeout=5)
                self.assertEqual(process.returncode, 143)
            self.assert_cleaned(app)

    def test_invalid_surface_order_fails_without_starting_children(self):
        for args in (['--surfaces'], ['--surfaces', 'zone', 'extra'], ['--surfaces', 'other']):
            with self.subTest(args=args), tempfile.TemporaryDirectory() as directory:
                app = pathlib.Path(directory)
                self.stage_processes(app)
                with self.launch(app, *args) as process:
                    process.communicate(timeout=5)
                    self.assertEqual(process.returncode, 2)
                self.assertFalse((app / 'brain.pid').exists())

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
            (stage / 'bin/korri-webos').write_bytes(b'launcher')
            files = payload_files(stage, '0.0.0')
            destination = pathlib.Path(directory) / 'data.tar.gz'
            write_payload(destination, files)
            with tarfile.open(destination) as archive:
                self.assertEqual(archive.getmember(APP_BASE + 'bin/korri-webos').mode, 0o755)
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
