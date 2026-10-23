"""Build gate: production sources must discover device-specific values."""

import ast
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]


def violations(text: str, python: bool = False) -> list[str]:
    patterns = {
        "address": r"(?<![\w.])(?:\d{1,3}\.){3}\d{1,3}(?![\w.])",
        "connector": r"\b(?:DSI|HDMI-A|DP|eDP|LVDS)-\d+\b",
        "encoder": r"\b(?:h264_v4l2m2m|h264_vaapi|h264_nvenc|nvenc|rkmpp|libx264)\b",
        "identity literal": r"(?:--(?:reuid|regid|groups)=|/run/user/)\d+",
        "refresh literal": r"(?im)(?:refresh(?:_hz|_nsec)?|fps)\s*=\s*(?:30|60|120)\b",
    }
    found = [name for name, pattern in patterns.items() if re.search(pattern, text)]
    if python:
        tree = ast.parse(text)
        fields = {
            "host",
            "address",
            "connector",
            "encoder",
            "uid",
            "gid",
            "user",
            "port",
            "refresh",
            "refresh_hz",
            "fps",
        }
        for node in ast.walk(tree):
            if (
                isinstance(node, ast.Assign)
                and isinstance(node.value, ast.Constant)
                and node.value.value is not None
            ):
                for target in node.targets:
                    if isinstance(target, ast.Name) and target.id.lower() in fields:
                        found.append("device field literal")
            if isinstance(node, ast.Dict):
                for key, value in zip(node.keys, node.values):
                    if (
                        isinstance(key, ast.Constant)
                        and key.value in fields
                        and isinstance(value, ast.Constant)
                        and value.value is not None
                    ):
                        found.append("device field literal")
            if (
                isinstance(node, ast.Call)
                and isinstance(node.func, ast.Attribute)
                and node.func.attr in ("setuid", "setgid")
            ):
                if node.args and isinstance(node.args[0], ast.Constant):
                    found.append("UID/GID constant")
    return found


class SourceCheck(unittest.TestCase):
    def test_production_contains_no_device_profile_constants(self):
        for path in ROOT.iterdir():
            if path.suffix in (".py", ".c", ".h"):
                with self.subTest(path=path.name):
                    self.assertEqual(
                        violations(path.read_text(), path.suffix == ".py"), []
                    )

    def test_semantic_literals_do_not_depend_on_known_device_values(self):
        for text in [
            "fps = 144",
            "port = 43210",
            'encoder = "software"',
            'user = "operator"',
            '{"uid": 2345}',
        ]:
            self.assertTrue(violations(text, python=True), text)

    def test_gate_detects_regressions(self):
        for text in [
            'host="10.42.5.1"',
            'output="DSI-1"',
            'encoder="h264_v4l2m2m"',
            "XDG_RUNTIME_DIR=/run/user/1000",
            "fps = 60",
            "refresh_hz=120",
            "os.setuid(1000)",
        ]:
            with self.subTest(text=text):
                self.assertTrue(violations(text, text.startswith("os.")))
