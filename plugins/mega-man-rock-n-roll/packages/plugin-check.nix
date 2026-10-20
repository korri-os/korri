# Package checks for the built plugin. Build for the target system on a build
# machine; nothing here downloads the game or the FEX rootfs.
{ pkgs }:
let
  plugin = import ./plugin.nix { inherit pkgs; };
  fexPlugin = import ../../fex/packages/plugin.nix { inherit pkgs; };
in
pkgs.runCommand "mega-man-rock-n-roll-plugin-package-check"
  {
    nativeBuildInputs = [ pkgs.python3 ];
  }
  ''
    python3 - ${plugin} ${fexPlugin} <<'PY'
    import json
    import os
    from pathlib import Path
    import re
    import struct
    import subprocess
    import sys
    import tempfile

    package, fex = map(Path, sys.argv[1:])
    manifest = json.loads((package / 'manifest.json').read_text())
    assert manifest['publisher'] == {'namespace': '@korri'}
    assert manifest['sources'] == ['plugin.ts']
    assert manifest['services'] == {}
    # The exact @korri:fex output whose config the runner reads.
    assert manifest['requires'] == [str(fex)], manifest['requires']
    fex_files = json.loads((fex / 'manifest.json').read_text())['files']
    assert sorted(fex_files) == ['config', 'fex-unpack'], fex_files
    files = manifest['files']
    assert sorted(files) == ['fex-launch', 'game-unpack'], files

    unpack = files['game-unpack']
    for arguments in ([], ['/nonexistent-work-dir'], ['/nonexistent-work-dir', '/nonexistent.zip']):
        result = subprocess.run([unpack, *arguments], capture_output=True)
        assert result.returncode == 1, (arguments, result)
        assert not result.stdout, result.stdout

    launch = files['fex-launch']
    # The bundled FEX is first on the launcher's PATH.
    search = re.search(r'export PATH="([^"]*)"', Path(launch).read_text()).group(1)
    interpreter = Path(search.split(':')[0]) / 'FEX'
    assert interpreter.is_file() and os.access(interpreter, os.X_OK), search
    assert subprocess.run([launch], capture_output=True).returncode == 2
    game = Path(tempfile.mkdtemp())
    (game / 'data.pck').write_bytes(b'pack')
    (game / 'MegaMan_RocknRoll').write_bytes(b'elf')
    home = tempfile.mkdtemp()
    runtime = {
        'HOME': home,
        'KORRI_FEX_CONFIG': fex_files['config'],
        'KORRI_FEX_ROOTFS': '/nonexistent/fex/rootfs/archlinux-2026-01-08-mesa-26.1.2',
    }
    result = subprocess.run([launch, str(game)], env=runtime, capture_output=True)
    assert result.returncode == 2, result.stderr
    assert b'KORRI_FEX_ROOTFS is not a directory' in result.stderr, result.stderr

    # The bundled FEX runs an x86_64 game executable with the @korri:fex
    # config. This hand-assembled static ELF writes one line and exits 0, so
    # it needs nothing from a rootfs.
    message = b'x86_64 game ran\n'
    code = (bytes.fromhex('b801000000bf01000000488d35') + struct.pack('<i', 16)
            + b'\xba' + struct.pack('<I', len(message))
            + bytes.fromhex('0f05b83c00000031ff0f05') + message)
    base, offset = 0x400000, 64 + 56
    elf = (b'\x7fELF' + bytes([2, 1, 1, 0]) + bytes(8)
           + struct.pack('<HHIQQQIHHHHHH', 2, 0x3e, 1, base + offset, 64, 0, 0, 64, 56, 1, 0, 0, 0)
           + struct.pack('<IIQQQQQQ', 1, 5, 0, base, base, offset + len(code), offset + len(code), 0x1000)
           + code)
    (game / 'MegaMan_RocknRoll').write_bytes(elf)
    runtime['KORRI_FEX_ROOTFS'] = tempfile.mkdtemp()
    result = subprocess.run([launch, str(game)], env=runtime, capture_output=True, timeout=120)
    assert result.returncode == 0, result
    assert result.stdout == message, result
    PY
    touch "$out"
  ''
