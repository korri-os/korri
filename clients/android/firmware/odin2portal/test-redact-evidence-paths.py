#!/usr/bin/env python3
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
MODULE_PATH = HERE / "redact-evidence-paths.py"
spec = importlib.util.spec_from_file_location("redact_evidence_paths", MODULE_PATH)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class RedactEvidencePathsTest(unittest.TestCase):
    def test_replaces_paths_as_fixed_strings(self):
        text = "source=/tmp/stock|copy/[slot]\\root\nother=/tmp/stockXcopy"
        result = module.redact_text(
            text,
            [("/tmp/stock|copy/[slot]\\root", "<SOURCE>")],
        )
        self.assertEqual(result, "source=<SOURCE>\nother=/tmp/stockXcopy")

    def test_rejects_empty_source(self):
        with self.assertRaisesRegex(ValueError, "must not be empty"):
            module.redact_text("text", [("", "replacement")])

    def test_cli_redacts_only_text_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            text = root / "verify.txt"
            binary = root / "key.avbpubkey"
            text.write_text("key=/private/key|name.pem\n", encoding="utf-8")
            binary.write_bytes(b"/private/key|name.pem")
            module.main([str(root), "/private/key|name.pem", "<PRIVATE-KEY>"])
            self.assertEqual(text.read_text(encoding="utf-8"), "key=<PRIVATE-KEY>\n")
            self.assertEqual(binary.read_bytes(), b"/private/key|name.pem")


if __name__ == "__main__":
    unittest.main()
