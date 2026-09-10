# Build-side extraction of the *pinned* NixOS OpenSSH module. No module is
# shipped for device evaluation. See README.md for every non-config effect.
{ pkgs }:
let
  lib = pkgs.lib;
  evaluated = import (pkgs.path + "/nixos/lib/eval-config.nix") {
    inherit pkgs;
    system = pkgs.stdenv.hostPlatform.system;
    modules = [
      {
        services.openssh = {
          enable = true;
          # Coexist with recovery SSH. Firewall metadata derives from this same
          # native NixOS option, never from a second hand-written manifest.
          ports = [ 2222 ];
          # Keys are supplied with sshd -h from the host's StateDirectory.
          hostKeys = [ ];
          generateHostKeys = false;
          settings = {
            # The native sshd_config owns these settings. Suppress the module's
            # password defaults; its PAM account/session support remains.
            PasswordAuthentication = null;
            KbdInteractiveAuthentication = null;
            ModuliFile = cfg.services.openssh.moduliFile;
          };
        };
      }
    ];
  };
  cfg = evaluated.config;
  # List all option definitions contributed by the upstream module, not only
  # the selected unit. This makes dropped effects visible on nixpkgs updates.
  module = import (pkgs.path + "/nixos/modules/services/networking/ssh/sshd.nix") {
    inherit lib pkgs;
    config = cfg;
  };
  definitions =
    path: node:
    if lib.isOption (lib.attrByPath path { } evaluated.options) then
      [ (lib.concatStringsSep "." path) ]
    else if node._type or "" == "merge" then
      lib.concatMap (definitions path) node.contents
    else if node._type or "" == "if" then
      lib.optionals node.condition (definitions path node.content)
    else if builtins.isAttrs node then
      lib.concatLists (lib.mapAttrsToList (name: value: definitions (path ++ [ name ]) value) node)
    else
      [ ];
  effects = import ./upstream-effects.nix;
  definedOptions = lib.unique (definitions [ ] module.config);
  reviewed =
    assert lib.assertMsg (
      builtins.sort builtins.lessThan definedOptions == builtins.attrNames effects
    ) "OpenSSH module contributions changed; review plugins/ssh/upstream-effects.nix";
    assert lib.assertMsg (builtins.all (a: a.assertion) (
      lib.concatMap (
        part: lib.optionals part.condition (part.content.assertions or [ ])
      ) module.config.contents
    )) "Pinned OpenSSH extraction failed a NixOS assertion";
    true;
  report =
    assert reviewed;
    pkgs.writeText "openssh-module-extraction.json" (
      builtins.toJSON {
        module = "nixos/modules/services/networking/ssh/sshd.nix";
        nixpkgsSource = builtins.unsafeDiscardStringContext (toString pkgs.path);
        inherit definedOptions effects;
        config = cfg.environment.etc."ssh/sshd_config".source;
        originalService = "${cfg.systemd.units."sshd.service".unit}/sshd.service";
        originalConnectionService = "${cfg.systemd.units."sshd@.service".unit}/sshd@.service";
        environmentFiles = builtins.filter (lib.hasPrefix "ssh/") (builtins.attrNames cfg.environment.etc);
        privilegeSeparationUser = lib.getAttrs [
          "name"
          "group"
          "isSystemUser"
          "description"
        ] cfg.users.users.sshd;
        pam = cfg.security.pam.services.sshd.text;
        rootProvisioning = cfg.systemd.tmpfiles.settings."ssh-root-provision";
        ports = cfg.networking.firewall.allowedTCPPorts;
        hostKeys = cfg.services.openssh.hostKeys;
        generateHostKeys = cfg.services.openssh.generateHostKeys;
        nssModulesPath = cfg.system.nssModules.path;
      }
    );
in
{
  inherit report;
  config =
    assert reviewed;
    cfg.environment.etc."ssh/sshd_config".source;
  ports.allowedTCPPorts = cfg.networking.firewall.allowedTCPPorts;
}
