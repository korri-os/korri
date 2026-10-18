# Proton compatibility tool files. plugin.ts names no artifact; a requiring
# runner reads these keys. The tool itself is not in the closure:
# cachyos-prepare downloads it on the device into korrid's cache root.
{ pkgs }:
let
  cachyos = import ./cachyos/pin.nix;
  compatibilitytool = pkgs.runCommand "korri-proton-cachyos-compatibilitytool" { } ''
    mkdir -p "$out/share"
    substitute ${./cachyos/compatibilitytool.vdf} "$out/share/compatibilitytool.vdf" \
      --replace-fail @toolName@ ${pkgs.lib.escapeShellArg cachyos.toolName}
  '';
  prepare = pkgs.writeShellApplication {
    name = "cachyos-prepare";
    runtimeInputs = with pkgs; [
      coreutils
      curl
      gnugrep
      gnutar
      util-linux
      xz
    ];
    text =
      pkgs.lib.replaceStrings
        [ "@url@" "@sha256@" "@version@" "@compatibilitytool@" ]
        [
          cachyos.url
          cachyos.sha256
          cachyos.version
          "${compatibilitytool}/share/compatibilitytool.vdf"
        ]
        (builtins.readFile ./cachyos/prepare.sh);
  };
in
{
  packages = {
    cachyos-prepare = prepare;
  };
  # cachyos-prepare installs compatibilitytool.vdf from its own closure.
  files = {
    cachyos-prepare = "${prepare}/bin/cachyos-prepare";
  };
}
