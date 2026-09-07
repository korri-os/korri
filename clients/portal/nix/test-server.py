#!/usr/bin/env nix
#! nix shell nixpkgs#python3 --command python3
"""Serve a changing profile path with the standard HTTP server for CLI tests."""

import functools
import http.server
import pathlib
import sys

handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=sys.argv[1])
with http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler) as server:
    pathlib.Path(sys.argv[2]).write_text(f"{server.server_port}\n")
    server.serve_forever()
