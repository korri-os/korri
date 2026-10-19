# Package checks for the built plugin. Build for the target system on a
# build machine; nothing here downloads the rootfs.
{ pkgs }:
let
  plugin = import ./plugin.nix { inherit pkgs; };
in
pkgs.runCommand "fex-plugin-package-check"
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
    assert 'requires' not in manifest
    files = manifest['files']
    assert sorted(files) == ['config', 'fex-unpack'], files
    config = Path(files['config'])
    assert json.loads((config / 'Config.json').read_text())['Config']['RootFS'] == 'ArchLinux'
    assert (config / 'AppConfig/steamwebhelper.json').is_file()
    unpack = files['fex-unpack']
    for arguments in ([], ['/nonexistent-work-dir'], ['/nonexistent-work-dir', '/nonexistent.sqsh']):
        result = subprocess.run([unpack, *arguments], capture_output=True)
        assert result.returncode == 1, (arguments, result)
        assert not result.stdout, result.stdout
    PY
    touch "$out"
  ''
