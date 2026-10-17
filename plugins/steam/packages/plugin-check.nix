{ pkgs }:
let
  plugin = import ./plugin.nix { inherit pkgs; };
in
pkgs.runCommand "steam-plugin-package-check"
  {
    nativeBuildInputs = [ pkgs.python3 ];
  }
  ''
    python3 - ${plugin} <<'PY'
    import json
    from pathlib import Path
    import subprocess
    import sys

    package = Path(sys.argv[1])
    manifest = json.loads((package / 'manifest.json').read_text())
    assert manifest['publisher'] == {'namespace': '@korri'}
    assert manifest['sources'] == ['plugin.ts']
    assert manifest['services'] == {}
    runner = manifest['files']['steam-session']
    assert Path(runner).is_file()
    assert subprocess.run([runner], capture_output=True).returncode == 2
    assert subprocess.run([runner, '401710;false'], capture_output=True).returncode == 2
    result = subprocess.run([runner, '401710'], env={}, capture_output=True)
    assert result.returncode == 2, result.stderr
    assert b'STEAM_HOME' in result.stderr
    PY
    touch "$out"
  ''
