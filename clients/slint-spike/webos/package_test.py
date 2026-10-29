#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#binutils --command python3
"""Exercise native archive headers without running a payload or contacting a TV."""
import pathlib
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
from package import write_ipk

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
