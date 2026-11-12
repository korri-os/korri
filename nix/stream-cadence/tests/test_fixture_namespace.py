"""The root-only, same-device route for a decoder that hardcodes X display :0."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import viewer  # noqa: E402
from cli import pc_fixture_namespace  # noqa: E402


class FixtureNamespace(unittest.TestCase):
    def test_only_same_device_private_product_pair_can_use_the_namespace(self):
        self.assertTrue(pc_fixture_namespace(product=True, fixture=True, private_pair=True, same_device=True, platform='x11_vaapi'))
        for change in ({'product': False}, {'fixture': False}, {'platform': 'x11'}):
            arguments = dict(product=True, fixture=True, private_pair=True, same_device=True, platform='x11_vaapi')
            arguments.update(change)
            self.assertFalse(pc_fixture_namespace(**arguments))
        for change in ({'same_device': False}, {'private_pair': False}):
            arguments = dict(product=True, fixture=True, private_pair=True, same_device=True, platform='x11_vaapi')
            arguments.update(change)
            with self.assertRaisesRegex(ValueError, 'same device.*:0'):
                pc_fixture_namespace(**arguments)

    def test_namespaced_product_fixture_has_only_its_private_x_display(self):
        session = object.__new__(viewer.Session)
        session.root = Path('/tmp/run')
        session.fixture = True
        session.plugin_viewer = True
        session.x_namespace = True
        environment = session.viewer_environment()
        self.assertEqual(environment['DISPLAY'], ':0')
        self.assertEqual(environment['SDL_VIDEODRIVER'], 'x11')
        self.assertEqual(environment['XDG_RUNTIME_DIR'], '/tmp/run/rt')
        self.assertNotIn('XAUTHORITY', environment)
        self.assertEqual(environment['SDL_AUDIODRIVER'], 'dummy')


class PrivateBus(unittest.TestCase):
    def test_private_fixture_does_not_autolaunch_dbus_or_reach_the_product_bus(self):
        session = object.__new__(viewer.Session)
        session.root = Path('/tmp/run')
        session.fixture = session.plugin_viewer = session.x_namespace = True
        environment = session.viewer_environment()
        for name in ('DBUS_SESSION_BUS_ADDRESS', 'DBUS_SYSTEM_BUS_ADDRESS'):
            self.assertTrue(environment[name].startswith('unix:path=/tmp/run/rt/'))
