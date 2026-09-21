{ korri }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.korriBundle;
  system = pkgs.stdenv.hostPlatform.system;
  activePath = "/nix/var/nix/gcroots/korri-bundle/active";
in
{
  options.services.korriBundle = {
    enable = lib.mkEnableOption "immutable Korri service bundle selection";
    initialPackage = lib.mkOption {
      type = lib.types.package;
      default = korri.packages.${system}.korri-bundle;
      defaultText = lib.literalExpression "korri.packages.${system}.korri-bundle";
      description = "Initial immutable bundle. Later bundle switches do not require NixOS activation.";
    };
    launcherPackage = lib.mkOption {
      type = lib.types.package;
      default = korri.packages.${system}.korri-inputd;
      defaultText = lib.literalExpression "korri.packages.${system}.korri-inputd";
      description = "Stable host package that validates and launches the selected bundle.";
    };
    activePath = lib.mkOption {
      type = lib.types.str;
      default = activePath;
      readOnly = true;
      description = "Root-owned active bundle selector.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.korri-bundle-selector = {
      description = "Initialize the immutable Korri service bundle selector";
      wantedBy = [ "multi-user.target" ];
      before = [
        "inputplumber.service"
        "korri-inputd.service"
        "korri-input-seat-receiver.service"
        "korri-local-signer.service"
        "korrid.service"
      ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        UMask = "0077";
        ExecStart = "${cfg.launcherPackage}/bin/korri-bundle-select initialize ${cfg.initialPackage}";
      };
    };
  };
}
