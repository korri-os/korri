# Evaluate the actual host module both with and without upstream recovery SSH.
# This is a configuration check, not a VM or a device configuration switch.
{
  pkgs,
  hostModule,
  hostPackage,
}:
let
  lib = pkgs.lib;
  evaluate =
    recovery: compatible: extra:
    (import (pkgs.path + "/nixos/lib/eval-config.nix") {
      inherit pkgs;
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        { services.openssh.enable = recovery; }
      ]
      ++ [ extra ]
      ++ lib.optionals compatible [
        hostModule
        {
          services.korri.pluginHost = {
            enable = true;
            package = hostPackage;
          };
        }
      ];
    }).config;
  cold = evaluate false true { };
  before = evaluate true false { };
  after = evaluate true true { };
  noPamRecovery = evaluate true true { services.openssh.settings.UsePAM = false; };
  noPamBefore = evaluate true false { services.openssh.settings.UsePAM = false; };
  custom = {
    security.pam.services.sshd.text = "auth required pam_deny.so\naccount required pam_permit.so\n";
  };
  customCold = evaluate false true custom;
  customRecovery = evaluate true true (custom // { services.openssh.settings.UsePAM = false; });
  same = path: lib.getAttrFromPath path before == lib.getAttrFromPath path after;
  unchanged = [
    [
      "environment"
      "etc"
      "ssh/sshd_config"
      "source"
    ]
    [
      "security"
      "pam"
      "services"
      "sshd"
      "text"
    ]
    [
      "systemd"
      "services"
      "sshd"
      "serviceConfig"
    ]
    [
      "systemd"
      "services"
      "sshd"
      "environment"
    ]
    [
      "systemd"
      "services"
      "sshd-keygen"
      "script"
    ]
    [
      "services"
      "openssh"
      "hostKeys"
    ]
    [
      "systemd"
      "tmpfiles"
      "settings"
      "ssh-root-provision"
    ]
    [
      "networking"
      "firewall"
      "allowedTCPPorts"
    ]
  ];
in
assert noPamRecovery.security.pam.services ? sshd;
assert noPamRecovery.security.pam.services.sshd.startSession;
assert !noPamRecovery.security.pam.services.sshd.unixAuth;
assert noPamRecovery.services.openssh.settings.UsePAM == false;
assert
  noPamRecovery.environment.etc."ssh/sshd_config".source
  == noPamBefore.environment.etc."ssh/sshd_config".source;
assert
  noPamRecovery.systemd.services.sshd.serviceConfig
  == noPamBefore.systemd.services.sshd.serviceConfig;
assert
  noPamRecovery.systemd.services.sshd.environment == noPamBefore.systemd.services.sshd.environment;
assert
  noPamRecovery.networking.firewall.allowedTCPPorts
  == noPamBefore.networking.firewall.allowedTCPPorts;
assert customCold.security.pam.services.sshd.text == custom.security.pam.services.sshd.text;
assert customRecovery.security.pam.services.sshd.text == custom.security.pam.services.sshd.text;
assert !cold.services.openssh.enable;
assert !cold.services.openssh.generateHostKeys;
assert !(cold.environment.etc ? "ssh/sshd_config");
assert !(cold.systemd.services ? sshd);
assert !(cold.systemd.services ? sshd-keygen);
assert cold.users.users.sshd.isSystemUser;
assert cold.users.users.sshd.group == "sshd";
assert cold.users.groups ? sshd;
# Privilege separation also uses the empty chroot from NixOS base activation.
assert builtins.elem "D /var/empty 0555 root root -" cold.systemd.tmpfiles.rules;
assert cold.security.pam.services.sshd.startSession;
assert !cold.security.pam.services.sshd.unixAuth;
assert cold.networking.firewall.allowedTCPPorts == [ ];
assert lib.assertMsg (builtins.all same unchanged)
  "Compatible plugin host changed recovery SSH configuration";
pkgs.runCommand "korri-ssh-host-support" { } ''
  touch "$out"
''
