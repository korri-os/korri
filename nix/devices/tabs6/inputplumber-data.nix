# Controller data for the controllers paired with this tablet. It has a
# device files only: the GameSir-G8+ and the Razer Kishi V2 use upstream's
# dinput_generic map. The
# shared validator requires capability maps, so it checks this device file
# beside the pinned InputPlumber's own maps, which also proves the map exists.
{ pkgs, inputplumber }:

pkgs.runCommand "tabs6-inputplumber-data"
  {
    nativeBuildInputs = [
      (pkgs.python3.withPackages (p: [
        p.pyyaml
        p.jsonschema
      ]))
    ];
  }
  ''
    upstream=${inputplumber}/share/inputplumber
    test -f "$upstream/capability_maps/dinput_generic.yaml"
    grep -qx 'id: dinput_generic' "$upstream/capability_maps/dinput_generic.yaml"
    mkdir check
    cp -r ${./inputplumber/devices} check/devices
    ln -s "$upstream/capability_maps" check/capability_maps
    python3 ${../../base/inputplumber-data-check.py} check "$upstream/schema"
    mkdir -p "$out/share/inputplumber"
    cp -r ${./inputplumber}/. "$out/share/inputplumber/"
  ''
