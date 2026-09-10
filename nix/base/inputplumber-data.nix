# Package the existing device-data layout and validate it against the runtime
# that consumes it. Physical button expectations remain with each device.
{
  pkgs,
  name,
  src,
  inputplumber,
  deviceCheck ? null,
}:

pkgs.runCommand name
  {
    nativeBuildInputs = [
      (pkgs.python3.withPackages (p: [
        p.pyyaml
        p.jsonschema
      ]))
    ];
  }
  ''
    mkdir -p "$out/share/inputplumber"
    cp -r ${src}/. "$out/share/inputplumber/"
    python3 ${./inputplumber-data-check.py} "$out/share/inputplumber" \
      ${inputplumber}/share/inputplumber/schema
    ${pkgs.lib.optionalString (deviceCheck != null) ''
      python3 ${deviceCheck} "$out/share/inputplumber"
    ''}
  ''
