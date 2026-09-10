{
  pkgs,
  inputplumber,
  dataPackage,
}:
pkgs.runCommand "korri-inputplumber-data-check"
  {
    nativeBuildInputs = [
      (pkgs.python3.withPackages (p: [
        p.pyyaml
        p.jsonschema
      ]))
    ];
  }
  ''
    cp ${./inputplumber-data-check.py} inputplumber-data-check.py
    cp ${./inputplumber-data-check.test.py} inputplumber-data-check.test.py
    python3 inputplumber-data-check.test.py ${dataPackage}/share/inputplumber \
      ${inputplumber}/share/inputplumber/schema
    touch "$out"
  ''
