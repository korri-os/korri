# korrid's tests read a committed example of a generated core plugin. This
# check proves the committed file is still exactly what the generator emits, so
# the fixture cannot drift away from what a device installs.
{ pkgs, mkPlugin }:
let
  libretro = import ./default.nix { inherit pkgs mkPlugin; };
  generated = libretro.packages."korri-plugin-mgba".generatedPlugin;
  committed = ../../services/korrid/examples/libretro-core.plugin.ts;
in
pkgs.runCommand "korri-libretro-example-check" { } ''
  if ! diff -u ${committed} ${generated}; then
    echo "services/korrid/examples/libretro-core.plugin.ts is stale." >&2
    echo "Regenerate it from the mgba catalogue entry." >&2
    exit 1
  fi
  touch "$out"
''
