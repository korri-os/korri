#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p "python3.withPackages (p: [ p.pyyaml p.jsonschema ])"
"""Validate device data against the schemas shipped by the selected InputPlumber."""

import json
from pathlib import Path
import sys

import jsonschema
import yaml

if len(sys.argv) != 3:
    sys.exit(f"Usage: {sys.argv[0]} <inputplumber-data> <schemas>")
root, schemas = map(Path, sys.argv[1:])
# Both device trees name these schemas in their yaml-language-server headers.
for directory, schema_name in [
    ("devices", "composite_device_v1.json"),
    ("capability_maps", "capability_map_v2.json"),
]:
    schema = json.loads((schemas / schema_name).read_text())
    files = sorted((root / directory).glob("*.yaml"))
    if not files:
        raise ValueError(f"No {directory} YAML files in {root}")
    for path in files:
        jsonschema.validate(yaml.safe_load(path.read_text()), schema)
        print(f"Validated {path.name} against {schema_name}")
