{ pkgs, inputplumber }:

pkgs.runCommand "rg353m-inputplumber-data"
  {
    nativeBuildInputs = [
      (pkgs.python3.withPackages (p: [ p.pyyaml p.jsonschema ]))
    ];
  }
  ''
    mkdir -p "$out/share/inputplumber"
    cp -r ${./inputplumber}/. "$out/share/inputplumber/"
    python3 ${./inputplumber-check.py} "$out/share/inputplumber" \
      ${inputplumber}/share/inputplumber/schema
  ''
