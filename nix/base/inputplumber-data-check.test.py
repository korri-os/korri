#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p "python3.withPackages (p: [ p.pyyaml p.jsonschema ])"
"""Exercise the validator with installed data and corrupted copies."""

from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

import yaml

source, schemas = map(Path, sys.argv[1:])
validator = Path(__file__).with_name("inputplumber-data-check.py")


def validate(root):
    return subprocess.run(
        [sys.executable, str(validator), str(root), str(schemas)],
        text=True,
        capture_output=True,
        timeout=30,
    )


usage = subprocess.run(
    [sys.executable, str(validator)], text=True, capture_output=True, timeout=30
)
assert usage.returncode != 0 and "Usage:" in usage.stderr, usage.stderr
result = validate(source)
assert result.returncode == 0, result.stderr
with tempfile.TemporaryDirectory() as temporary:
    root = Path(temporary) / "inputplumber"
    shutil.copytree(source, root)
    root.chmod(0o755)
    for path in root.rglob("*"):
        path.chmod(0o755 if path.is_dir() else 0o644)
    profile_path = next((root / "devices").glob("*.yaml"))
    profile = yaml.safe_load(profile_path.read_text())
    profile["target_devices"] = ["invalid-target"]
    profile_path.write_text(yaml.safe_dump(profile))
    result = validate(root)
    assert result.returncode != 0, "Invalid target device passed validation"
    assert "ValidationError" in result.stderr, result.stderr
    shutil.rmtree(root / "devices")
    result = validate(root)
    assert result.returncode != 0, "Missing device files passed validation"
    assert "No devices YAML files" in result.stderr, result.stderr
    shutil.copytree(source / "devices", root / "devices")
    (root / "devices").chmod(0o755)
    capability_path = next((root / "capability_maps").glob("*.yaml"))
    capabilities = yaml.safe_load(capability_path.read_text())
    capabilities["mapping"] = "not-a-list"
    capability_path.write_text(yaml.safe_dump(capabilities))
    result = validate(root)
    assert result.returncode != 0, "Invalid capability mappings passed validation"
    assert "ValidationError" in result.stderr, result.stderr
    shutil.rmtree(root / "capability_maps")
    result = validate(root)
    assert result.returncode != 0, "Missing capability maps passed validation"
    assert "No capability_maps YAML files" in result.stderr, result.stderr
print(
    "InputPlumber validation accepts installed data and rejects invalid or missing data"
)
