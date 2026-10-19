# Proton compatibility tool files. plugin.ts declares the cachyos artifact
# and names `cachyos-unpack` as its unpack program. The tool itself is not
# in the closure: korrid downloads the release plugin.ts pins into its cache
# and runs cachyos-unpack on it.
{ pkgs }:
let
  cachyos = import ./cachyos/tool-name.nix;
  compatibilitytool = pkgs.runCommand "korri-proton-cachyos-compatibilitytool" { } ''
    mkdir -p "$out/share"
    substitute ${./cachyos/compatibilitytool.vdf} "$out/share/compatibilitytool.vdf" \
      --replace-fail @toolName@ ${pkgs.lib.escapeShellArg cachyos.toolName}
  '';
  unpack = pkgs.writeShellApplication {
    name = "cachyos-unpack";
    runtimeInputs = with pkgs; [
      coreutils
      gnugrep
      gnutar
      xz
    ];
    text =
      pkgs.lib.replaceStrings [ "@compatibilitytool@" ]
        [ "${compatibilitytool}/share/compatibilitytool.vdf" ]
        (builtins.readFile ./cachyos/unpack.sh);
  };
in
{
  packages = {
    cachyos-unpack = unpack;
  };
  # cachyos-unpack installs compatibilitytool.vdf from its own closure.
  files = {
    cachyos-unpack = "${unpack}/bin/cachyos-unpack";
  };
}
