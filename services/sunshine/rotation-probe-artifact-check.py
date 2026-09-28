#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#binutils --command python3
"""Check local experimental ARM plugin output identity; not a signature gate."""
import json
from pathlib import Path
import subprocess
import sys

if len(sys.argv) != 3:
    raise SystemExit(f'usage: {sys.argv[0]} ON_PLUGIN OFF_PLUGIN')
expected_hash = __import__('hashlib').sha256((Path(__file__).parent / 'rotation-probe.patch').read_bytes()).hexdigest()
for mode, raw_path in [('on', sys.argv[1]), ('off', sys.argv[2])]:
    plugin = Path(raw_path).resolve(strict=True)
    manifest = json.loads((plugin / 'manifest.json').read_text())
    assert manifest['publisher']['namespace'] == '@korri'
    assert set(manifest['services']) == {
        'korri-sunshine', 'korri-sunshine-input-setup', 'korri-sunshine-certificate-control.socket'
    }
    package = Path(manifest['packages']['sunshine']).resolve(strict=True)
    assert manifest['files']['sunshine'] == str(package / 'bin/sunshine')
    assert 'sunshine-rotation-experiment-' in package.name
    assert package.name.endswith(f'-probe-{mode}')
    provenance = dict(line.split('=', 1) for line in (package / 'share/korri/sunshine-korri/provenance').read_text().splitlines() if '=' in line)
    assert provenance['package'] == 'sunshine-rotation-experiment'
    assert provenance['build_profile'] == 'aarch64-linux-rkmpp-v4l2m2m'
    assert provenance['experimental_parent_profile'] == provenance['build_profile']
    assert provenance['experimental_rotation_probe'] == '1'
    assert provenance['experimental_rotation_patch_sha256'] == expected_hash
    assert provenance['experimental_rotation_force_off'] == ('1' if mode == 'off' else '0')
    assert provenance['approved_parent_patch_set_sha256'] != expected_hash
    result = subprocess.run(['readelf', '-h', str(package / 'bin/sunshine')], capture_output=True, text=True, check=True)
    assert 'Machine:                           AArch64' in result.stdout
    print(f'{mode}: ARM plugin {plugin}, experimental package {package}, patch {expected_hash}')
print('local artifact identity passed; signatures, device rendering and timing unverified')
