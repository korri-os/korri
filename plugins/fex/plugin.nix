# FEX runtime files. plugin.ts declares the rootfs artifact and names
# `fex-unpack` as its unpack program; a requiring runner reads `config`. The
# rootfs is not in the closure: korrid downloads the files plugin.ts pins
# into its cache and runs fex-unpack on them.
{ pkgs }:
let
  unpack = pkgs.writeShellApplication {
    name = "fex-unpack";
    runtimeInputs = with pkgs; [
      coreutils
      gnutar
      squashfsTools
      zstd
    ];
    text = builtins.readFile ./rootfs/unpack.sh;
  };
  config = pkgs.runCommand "korri-fex-config" { } ''
    mkdir -p "$out/share"
    cp -r ${./config} "$out/share/fex-emu"
  '';
in
{
  packages = {
    fex-unpack = unpack;
    fex-config = config;
  };
  files = {
    fex-unpack = "${unpack}/bin/fex-unpack";
    config = "${config}/share/fex-emu";
  };
}
