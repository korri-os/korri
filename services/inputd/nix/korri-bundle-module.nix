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
      # The launcher starts InputPlumber from the active bundle, not from the
      # service's XDG_DATA_DIRS. Default to this device's own InputPlumber
      # package, which carries its built-in controller files on top of the
      # generic list, so every device's controller data reaches the provider.
      default = import ./korri-bundle.nix {
        inherit pkgs;
        inputdPackage = korri.packages.${system}.korri-inputd;
        inputplumberKorri = config.services.inputplumber.package;
        korridPackage = korri.packages.${system}.korrid;
      };
      defaultText = lib.literalMD "a bundle of this flake's inputd and korrid with `services.inputplumber.package`";
      description = "Initial immutable bundle. Later bundle switches do not require NixOS activation.";
    };
    carriesDeviceInputPlumber = lib.mkOption {
      type = lib.types.bool;
      readOnly = true;
      default =
        toString (cfg.initialPackage.inputplumber or "")
        == toString config.services.inputplumber.package;
      defaultText = lib.literalMD "whether `initialPackage` starts `services.inputplumber.package`";
      description = "Whether the initial bundle starts this device's own InputPlumber data.";
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
    assertions = [
      {
        assertion = !config.services.inputplumber.enable || cfg.carriesDeviceInputPlumber;
        message = "services.korriBundle.initialPackage must start this device's services.inputplumber.package; otherwise its built-in controller files never reach InputPlumber.";
      }
    ];
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
      environment.KORRI_BUNDLE_INITIAL_PACKAGE = toString cfg.initialPackage;
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        UMask = "0077";
        ExecStart = "${cfg.launcherPackage}/bin/korri-bundle-select initialize \${KORRI_BUNDLE_INITIAL_PACKAGE}";
      };
    };
  };
}
