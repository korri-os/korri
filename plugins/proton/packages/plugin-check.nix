# Package checks for the built plugin. Build for the target system on a
# build machine; nothing here downloads Proton.
{ pkgs }:
let
  plugin = import ./plugin.nix { inherit pkgs; };
  cachyos = import ../cachyos/pin.nix;
in
pkgs.runCommand "proton-plugin-package-check"
  {
    nativeBuildInputs = [ pkgs.python3 ];
  }
  ''
    python3 - ${plugin} ${cachyos.toolName} <<'PY'
    import json
    from pathlib import Path
    import re
    import subprocess
    import sys

    package, tool = Path(sys.argv[1]), sys.argv[2]
    manifest = json.loads((package / 'manifest.json').read_text())
    assert manifest['publisher'] == {'namespace': '@korri'}
    assert manifest['sources'] == ['plugin.ts']
    assert manifest['services'] == {}
    assert 'requires' not in manifest
    files = manifest['files']
    assert sorted(files) == ['cachyos-prepare'], files
    prepare = files['cachyos-prepare']
    # The program installs the vdf it references from its own closure.
    vdfs = re.findall(r'/nix/store/[^"]+/compatibilitytool[.]vdf', Path(prepare).read_text())
    assert len(vdfs) == 1, vdfs
    vdf = Path(vdfs[0]).read_text()
    assert f'"{tool}"' in vdf and '@' not in vdf, vdf
    for arguments in ([], ['relative'], ['/nonexistent-cache-root']):
        result = subprocess.run([prepare, *arguments], capture_output=True)
        assert result.returncode == 2, (arguments, result)
        assert not result.stdout, result.stdout
    PY
    touch "$out"
  ''
