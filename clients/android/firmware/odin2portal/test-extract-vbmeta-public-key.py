#!/usr/bin/env python3
import importlib.util
import struct
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
MODULE_PATH = HERE / "extract-vbmeta-public-key.py"
spec = importlib.util.spec_from_file_location("extract_vbmeta_public_key", MODULE_PATH)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ExtractVbmetaPublicKeyTest(unittest.TestCase):
    def make_image(self, public_key: bytes) -> bytes:
        authentication_size = 16
        auxiliary_size = 64
        public_key_offset = 8
        header = bytearray(256)
        header[:4] = b"AVB0"
        struct.pack_into(">Q", header, 12, authentication_size)
        struct.pack_into(">Q", header, 20, auxiliary_size)
        struct.pack_into(">Q", header, 64, public_key_offset)
        struct.pack_into(">Q", header, 72, len(public_key))
        auxiliary = bytearray(auxiliary_size)
        auxiliary[public_key_offset : public_key_offset + len(public_key)] = public_key
        return bytes(header) + bytes(authentication_size) + bytes(auxiliary)

    def test_extracts_public_key_from_auxiliary_block(self):
        expected = b"fixture-avb-public-key"
        self.assertEqual(module.extract_public_key(self.make_image(expected)), expected)

    def test_rejects_wrong_magic(self):
        with self.assertRaisesRegex(ValueError, "not an AVB vbmeta image"):
            module.extract_public_key(bytes(256))

    def test_rejects_public_key_outside_auxiliary_block(self):
        image = bytearray(self.make_image(b"key"))
        struct.pack_into(">Q", image, 72, 100)
        with self.assertRaisesRegex(ValueError, "invalid AVB public-key bounds"):
            module.extract_public_key(bytes(image))

    def test_cli_writes_only_the_key(self):
        expected = b"fixture-key"
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "vbmeta.img"
            output = Path(directory) / "key.avbpubkey"
            source.write_bytes(self.make_image(expected))
            module.main([str(source), str(output)])
            self.assertEqual(output.read_bytes(), expected)


if __name__ == "__main__":
    unittest.main()
