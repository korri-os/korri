{ pkgs }:
let
  plugin = import ./plugin.nix { inherit pkgs; };
  fexPlugin = import ../../fex/packages/plugin.nix { inherit pkgs; };
  protonPlugin = import ../../proton/packages/plugin.nix { inherit pkgs; };
in
pkgs.runCommand "steam-plugin-package-check"
  {
    nativeBuildInputs = [ pkgs.python3 ];
  }
  ''
    python3 - ${plugin} ${fexPlugin} ${protonPlugin} <<'PY'
    import json
    from pathlib import Path
    import subprocess
    import sys
    import tempfile

    package, fex, proton = map(Path, sys.argv[1:])
    manifest = json.loads((package / 'manifest.json').read_text())
    assert manifest['publisher'] == {'namespace': '@korri'}
    assert manifest['sources'] == ['plugin.ts']
    assert manifest['services'] == {}
    # Exact outputs of the plugins whose files plugin.ts passes to the session.
    assert manifest['requires'] == [str(fex), str(proton)], manifest['requires']
    for required, keys in ((fex, ['config', 'fex-prepare']), (proton, ['cachyos-prepare'])):
        files = json.loads((required / 'manifest.json').read_text())['files']
        assert sorted(files) == keys, files
    runner = manifest['files']['steam-session']
    assert Path(runner).is_file()
    assert subprocess.run([runner], capture_output=True).returncode == 2
    assert subprocess.run([runner, '401710;false'], capture_output=True).returncode == 2
    result = subprocess.run([runner, '401710'], env={}, capture_output=True)
    assert result.returncode == 2, result.stderr
    assert b'STEAM_HOME' in result.stderr
    home = tempfile.mkdtemp()
    result = subprocess.run([runner, '401710'], env={'STEAM_HOME': home, 'HOME': home},
                            capture_output=True)
    assert result.returncode == 2, result.stderr
    assert b'KORRI_CACHE_ROOT must be an absolute path' in result.stderr, result.stderr
    fex_files = json.loads((fex / 'manifest.json').read_text())['files']
    proton_files = json.loads((proton / 'manifest.json').read_text())['files']
    runtime = {
        'STEAM_HOME': home, 'HOME': home, 'KORRI_CACHE_ROOT': home,
        'KORRI_FEX_PREPARE': '/nonexistent/fex-prepare',
        'KORRI_FEX_CONFIG': fex_files['config'],
        'KORRI_PROTON_CACHYOS_PREPARE': proton_files['cachyos-prepare'],
    }
    result = subprocess.run([runner, '401710'], env=runtime, capture_output=True)
    assert result.returncode == 2, result.stderr
    assert b'runtime file is missing' in result.stderr, result.stderr
    PY
    touch "$out"
  ''
