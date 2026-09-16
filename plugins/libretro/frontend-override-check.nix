# Proves a catalogue entry can name its own frontend build.
#
# The claim the design makes is narrow and worth holding: another pinned
# derivation can replace the frontend for one core without replacing any other
# plugin, and the settings evidence follows the selected binary rather than the
# default one. This check builds three core plugins and reads their manifests,
# so the claim is about what a device installs, not about what evaluates.
{ pkgs, mkPlugin }:
let
  corePlugin = import ./core-plugin.nix { inherit pkgs mkPlugin; };
  cores = import ./cores.nix { inherit pkgs; };

  # Stands in for a pinned fork: a real RetroArch derivation that is not the
  # default build. An override selects the build and never the protections, so
  # Korri's read-only patch and its compiled source regression still apply.
  pinned = pkgs.retroarch-bare.overrideAttrs (_: {
    pname = "retroarch-pinned";
  });

  default = corePlugin "mgba" cores.mgba;
  overridden = corePlugin "mgba" (cores.mgba // { frontend = pinned; });
  untouched = corePlugin "gambatte" cores.gambatte;
in
pkgs.runCommand "korri-frontend-override-check"
  {
    nativeBuildInputs = [ pkgs.buildPackages.python3 ];
  }
  ''
    python3 ${./frontend-override-check.py} \
      ${default}/manifest.json \
      ${overridden}/manifest.json \
      ${untouched}/manifest.json
    touch "$out"
  ''
