# FEX runtime files. plugin.ts names no artifact; a requiring runner reads
# these keys. The rootfs is not in the closure: fex-prepare downloads it on
# the device into korrid's cache root.
{ pkgs }:
let
  pins = import ./rootfs/pins.nix;
  pinLines = map (pin: "overlay ${pin.sha256} ${pin.url}\n") pins.overlay;
  pinFile = pkgs.writeText "fex-rootfs-pins" (
    pkgs.lib.concatStrings ([ "base ${pins.base.sha256} ${pins.base.url}\n" ] ++ pinLines)
  );
  prepare = pkgs.writeShellApplication {
    name = "fex-prepare";
    runtimeInputs = with pkgs; [
      coreutils
      curl
      gnutar
      squashfsTools
      util-linux
      zstd
    ];
    text = pkgs.lib.replaceStrings [ "@pins@" "@version@" ] [ "${pinFile}" pins.version ] (
      builtins.readFile ./rootfs/prepare.sh
    );
  };
  config = pkgs.runCommand "korri-fex-config" { } ''
    mkdir -p "$out/share"
    cp -r ${./config} "$out/share/fex-emu"
  '';
in
{
  packages = {
    fex-prepare = prepare;
    fex-config = config;
  };
  files = {
    fex-prepare = "${prepare}/bin/fex-prepare";
    config = "${config}/share/fex-emu";
  };
}
