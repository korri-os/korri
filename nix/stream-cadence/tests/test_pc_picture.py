"""PC selection follows the product without turning a PC into a handheld."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import viewer  # noqa: E402


class PcPicture(unittest.TestCase):
    def test_only_absent_device_tree_on_x86_with_existing_render_node_gets_vaapi(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            compatible, dri = root / 'compatible', root / 'dri'
            select = lambda arch: viewer.picture_path_from_path(compatible, arch, dri)
            self.assertEqual(select('x86_64'), 'x11')
            dri.mkdir()
            for name in ('card0', 'renderD', 'renderDx', 'renderD128extra', 'renderD１２８'):
                (dri / name).touch()
            (dri / 'renderD129').symlink_to(root / 'missing')
            self.assertEqual(select('x86_64'), 'x11')
            (dri / 'renderD128').touch()
            self.assertEqual(select('x86_64'), 'x11_vaapi')
            for arch in ('aarch64', 'arm', 'riscv64'):
                self.assertEqual(select(arch), 'x11')
            for value, expected in ((b'', 'x11'), (b'other\0', 'x11'), (b'rockchip,rk3566\0', 'v4l2request')):
                compatible.write_bytes(value)
                self.assertEqual(select('x86_64'), expected)
            compatible.unlink()
            compatible.mkdir()
            self.assertEqual(select('x86_64'), 'x11')

    def test_pc_vaapi_does_not_enable_handheld_guard(self):
        import heat
        import sender
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            old = viewer.COMPATIBLE, viewer.RENDER_NODES, heat.THERMAL
            viewer.COMPATIBLE, viewer.RENDER_NODES, heat.THERMAL = root / 'dt', root / 'dri', root / 'thermal'
            viewer.RENDER_NODES.mkdir()
            (viewer.RENDER_NODES / 'renderD128').touch()
            try:
                session = viewer.Session('/unused')
                self.assertEqual(session.facts['platform'], 'x11_vaapi')
                self.assertFalse(session.hardware_path())
                self.assertFalse(sender.Session().hardware_path)
                self.assertIsNone(viewer.no_fixture_on_board('viewer fixture'))
            finally:
                viewer.COMPATIBLE, viewer.RENDER_NODES, heat.THERMAL = old
