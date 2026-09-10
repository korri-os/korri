{
  config,
  lib,
  pkgs,
  ...
}:

let
  rockchipMpp = pkgs.callPackage ./rockchip-mpp.nix { };
  ffmpegRockchip = pkgs.callPackage ./ffmpeg-rockchip.nix {
    inherit rockchipMpp;
  };
in
{
  boot = {
    extraModulePackages = lib.mkAfter [
      (config.boot.kernelPackages.callPackage ./rk-mpp-service-module.nix { })
    ];
    kernelModules = [ "rk_vcodec" ];
  };

  hardware.deviceTree.overlays = lib.mkAfter [
    {
      name = "rg353m-rkvenc-mpp";
      dtsFile = ./rk-mpp-service-overlay.dts;
    }
  ];

  environment.systemPackages = [
    rockchipMpp
    ffmpegRockchip
  ];

  services.udev.extraRules = ''
    KERNEL=="mpp_service", GROUP="video", MODE="0660"
  '';
}
