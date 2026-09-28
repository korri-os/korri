#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#binutils --command python3
"""Check local experimental ARM plugin output identity; not a signature gate."""
import json
from pathlib import Path
import subprocess
import sys

if len(sys.argv) != 4:
    raise SystemExit(f'usage: {sys.argv[0]} ON_PLUGIN OFF_PLUGIN BASELINE_KORRI_SOURCE')
baseline_unit = Path(sys.argv[3]).resolve(strict=True) / 'plugins/sunshine/korri-sunshine.service'
assert baseline_unit.is_file(), 'missing Mini plugin producer'
expected_hash = __import__('hashlib').sha256((Path(__file__).parent / 'rotation-probe.patch').read_bytes()).hexdigest()
for mode, raw_path in [('on', sys.argv[1]), ('off', sys.argv[2])]:
    plugin = Path(raw_path).resolve(strict=True)
    manifest = json.loads((plugin / 'manifest.json').read_text())
    assert manifest['publisher']['namespace'] == '@korri'
    assert set(manifest['services']) == {
        'korri-sunshine', 'korri-sunshine-input-seat-receiver', 'korri-sunshine-certificate-control.socket'
    }
    # These paths belong to the Mini's installed, signed plugin. Only the
    # Sunshine binary and its wrapper/unit are allowed to change in this trial.
    assert manifest['packages']['inputd'] == '/nix/store/063zkyf78qyjj2ncr5hpim3069824cfv-korri-inputd-0.0.0'
    assert manifest['files']['input-seat-receiver'] == '/nix/store/063zkyf78qyjj2ncr5hpim3069824cfv-korri-inputd-0.0.0/bin/korri-input-seat-receiver'
    assert manifest['files']['setup'] == '/nix/store/0ap474jw0a9vq9x4ml31vyiqwczqhz4v-korri-sunshine-input-seat-setup/bin/korri-sunshine-input-seat-setup'
    assert manifest['services']['korri-sunshine-input-seat-receiver'] == '/nix/store/s7ywxkjrl79r2dgmvvpskph0z8yg3zk3-korri-korri-sunshine-input-seat-receiver.service/lib/systemd/system/korri-sunshine-input-seat-receiver.service'
    assert manifest['services']['korri-sunshine-certificate-control.socket'] == '/nix/store/0scbqn2qj0prnrb0hh6f1gybmlxs8120-korri-korri-sunshine-certificate-control.socket/lib/systemd/system/korri-sunshine-certificate-control.socket'
    expected_unit = baseline_unit.read_text().replace('@runtime@', manifest['files']['runtime'])
    assert Path(manifest['services']['korri-sunshine']).read_text() == expected_unit, 'Sunshine native unit changed beyond its binary path'
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
