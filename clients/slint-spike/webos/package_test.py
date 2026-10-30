#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#binutils --command python3
"""Exercise native archive headers without running a payload or contacting a TV."""
import pathlib
import subprocess
import sys
import tarfile
import tempfile
import unittest

sys.dont_write_bytecode = True
from package import APP_ID, write_ipk, write_payload

NAMES = ('debian-binary', 'control.tar.gz', 'data.tar.gz')


def native_headers(filename):
    data = pathlib.Path(filename).read_bytes()
    assert data[:8] == b'!<arch>\n', 'archive magic differs'
    offset = 8
    for name in NAMES:
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


class NativeArchiveTests(unittest.TestCase):
    def test_raw_headers_and_odd_padding_round_trip_through_ar(self):
        with tempfile.TemporaryDirectory() as folder:
            directory = pathlib.Path(folder)
            contents = (b'2.0\n', b'odd', b'even')
            for name, content in zip(NAMES, contents):
                (directory / name).write_bytes(content)
            destination = directory / 'candidate.ipk'
            write_ipk(destination, directory)
            native_headers(destination)
            self.assertEqual(subprocess.check_output(['ar', 't', str(destination)], text=True).splitlines(), list(NAMES))
            for name, content in zip(NAMES, contents):
                self.assertEqual(subprocess.check_output(['ar', 'p', str(destination), name]), content)

    def test_payload_preserves_files_and_includes_only_their_parent_directories(self):
        app_base = 'usr/palm/applications/' + APP_ID + '/'
        package_base = 'usr/palm/packages/' + APP_ID + '/'
        files = [
            (app_base + 'appinfo.json', b'app metadata', 0o644),
            (app_base + 'icon.png', b'icon bytes', 0o644),
            (app_base + 'bin/korri-slint-spike', b'executable bytes', 0o755),
            (package_base + 'packageinfo.json', b'package metadata', 0o644),
        ]
        expected_directories = set()
        for name, _, _ in files:
            parts = name.split('/')
            expected_directories.update('/'.join(parts[:depth]) for depth in range(1, len(parts)))
        with tempfile.TemporaryDirectory() as folder:
            destination = pathlib.Path(folder) / 'data.tar.gz'
            write_payload(destination, files)
            with tarfile.open(destination, 'r:gz') as archive:
                members = archive.getmembers()
                directories = [member for member in members if member.isdir()]
                self.assertEqual({member.name.rstrip('/') for member in directories}, expected_directories)
                self.assertEqual(len(members), len(expected_directories) + len(files))
                self.assertEqual(members[:len(directories)], directories)
                self.assertTrue(all(member.isdir() or member.isfile() for member in members))
                for member in members:
                    self.assertEqual((member.uid, member.gid, member.mtime), (0, 0, 0))
                for member in directories:
                    self.assertEqual((member.mode, member.size), (0o755, 0))
                regular_files = [member for member in members if member.isfile()]
                self.assertEqual([member.name for member in regular_files], [name for name, _, _ in files])
                for name, content, mode in files:
                    self.assertEqual(archive.extractfile(name).read(), content)
                    self.assertEqual(archive.getmember(name).mode, mode)

    def test_existing_archive_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as folder:
            destination = pathlib.Path(folder) / 'existing.ipk'
            destination.write_bytes(b'keep')
            with self.assertRaises(FileExistsError):
                write_ipk(destination, pathlib.Path(folder))
            self.assertEqual(destination.read_bytes(), b'keep')


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--check-ipk':
        native_headers(sys.argv[2])
        print('NATIVE_RAW_HEADERS_PASS')
    else:
        unittest.main()
