#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 nix
"""Keep native signed-cache reads available for old-device rollback.

Catalog images do not call this route. Their command-boundary and actual
published-archive proofs live under nix/cache/ and nix/product/.
Usage: test_fetch_image_plugins.py SIGNED_PACKAGE PUBLISHER_PUBLIC_KEY
"""
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from threading import Thread
import unittest

PACKAGE = sys.argv.pop(1)
KEY = sys.argv.pop(1)


class SignedCacheRollbackTests(unittest.TestCase):
    def receive(self, key):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            cache = work / 'cache'
            subprocess.run(['nix', 'copy', '--to', cache.as_uri(), PACKAGE], check=True)
            server = ThreadingHTTPServer(('127.0.0.1', 0), partial(SimpleHTTPRequestHandler, directory=str(cache)))
            thread = Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                store = f'local?root={work}/receiver'
                command = ['nix', '--extra-experimental-features', 'nix-command',
                           '--option', 'substituters', '', '--option', 'extra-substituters', '',
                           '--option', 'trusted-public-keys', key,
                           '--option', 'extra-trusted-public-keys', '',
                           '--option', 'require-sigs', 'true', '--option', 'max-jobs', '0',
                           '--option', 'builders', '', '--option', 'fallback', 'false']
                # NIX_CONFIG supplements global nix.conf; it does not replace it.
                # Keep host-only settings out of this receiving store too.
                (work / 'nix-conf').mkdir()
                env = {k: v for k, v in os.environ.items() if not k.startswith('NIX_')}
                env.update({'NIX_REMOTE': '', 'NIX_CONF_DIR': str(work / 'nix-conf'),
                            'NIX_USER_CONF_FILES': '/dev/null'})
                result = subprocess.run([*command, 'copy', '--from', f'http://127.0.0.1:{server.server_port}',
                                         '--to', store, PACKAGE], capture_output=True, text=True,
                                        env=env)
                self.assertNotIn('unknown setting', result.stderr,
                                 'host Nix configuration leaked into the fixture')
                if result.returncode == 0:
                    info = subprocess.check_output([*command, 'path-info', '--store', store, '--json', PACKAGE],
                                                   text=True, env=env)
                    self.assertTrue(json.loads(info)[PACKAGE]['signatures'])
                    self.assertTrue((work / 'receiver' / PACKAGE.lstrip('/')).exists())
                return result
            finally:
                server.shutdown()
                thread.join()
                server.server_close()

    def test_native_nix_receives_the_bound_signed_package_without_builds(self):
        result = self.receive(KEY)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_another_key_cannot_authorize_the_same_input_addressed_path(self):
        result = self.receive('other:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('signature', result.stderr.lower())


if __name__ == '__main__':
    unittest.main()
