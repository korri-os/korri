"""Actual decode is a condition, not an inference from an open OpenGL render node."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from viewer import decode_path  # noqa: E402
from verdict import judge  # noqa: E402


class DecodePath(unittest.TestCase):
    def test_pc_fallback_is_software_even_with_an_open_render_node(self):
        log = '[VAAPI @ ...] No VA display found for device :0.\nUsing FFmpeg decoder: h264\n'
        self.assertEqual(decode_path(log, 'x11_vaapi', ['/dev/dri/renderD129']), 'software')

    def test_vaapi_needs_the_positive_log_and_a_render_node(self):
        log = 'Platform X Window System (VAAPI)\nUsing FFmpeg decoder: h264\n'
        self.assertEqual(decode_path(log, 'x11_vaapi', ['/dev/dri/renderD128']), 'vaapi')
        for nodes in (None, []):
            self.assertEqual(decode_path(log, 'x11_vaapi', nodes), 'unknown')
        self.assertEqual(decode_path(log + 'No VA display found for device :0.\n', 'x11_vaapi', ['/dev/dri/renderD128']), 'unknown')

    def test_an_ordinary_decoder_name_alone_does_not_prove_software_on_vaapi(self):
        self.assertEqual(decode_path('Using FFmpeg decoder: h264\n', 'x11_vaapi', ['/dev/dri/renderD128']), 'unknown')
        self.assertEqual(decode_path('Using FFmpeg decoder: h264\n', 'sdl', ['/dev/dri/renderD128']), 'software')
        self.assertEqual(decode_path('No VA display found for device :0.\n', 'x11_vaapi', []), 'unknown')
        self.assertEqual(decode_path('Using FFmpeg decoder: h264_v4l2m2m\n', 'sdl', []), 'unknown')


class DecodeConditions(unittest.TestCase):
    def result(self, decode='software'):
        window = dict(latency_ms={'p50': 12, 'p95': 20}, capture_misses=0, repeated_frames=0, late_frames=0, torn_frames=0, undecodable_frames=0)
        record = {'discovered': {'valid': True, 'viewer_ran': {'kind': 'copied-product', 'place': 'fixture', 'moonlight': '/viewer', 'decode': decode, 'requested_platform': 'x11_vaapi'}}, 'source_counter': {'windows': [window]}, 'viewer': {'startup_ms': 700}}
        return [copy.deepcopy(record) for _ in range(5)]

    def test_same_decode_compares_but_different_decode_never_does_even_with_override(self):
        self.assertEqual(len(judge(self.result(), self.result(), 5)), 15)
        for override in (False, True):
            with self.assertRaisesRegex(ValueError, 'decode'):
                judge(self.result(), self.result('vaapi'), 5, override)

    def test_unknown_and_missing_decode_always_refuse(self):
        unknown = self.result('unknown')
        missing = self.result()
        for row in missing:
            del row['discovered']['viewer_ran']['decode']
        for rows, message in ((unknown, 'unknown'), (missing, 'recorded before the tool kept decode')):
            for override in (False, True):
                with self.subTest(message=message, override=override):
                    with self.assertRaisesRegex(ValueError, message):
                        judge(rows, rows, 5, override)
        mixed = self.result()
        mixed[-1]['discovered']['viewer_ran']['decode'] = 'vaapi'
        with self.assertRaisesRegex(ValueError, 'decode'):
            judge(mixed, self.result(), 5)
