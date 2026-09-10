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
    recovery: compatible:
    (import (pkgs.path + "/nixos/lib/eval-config.nix") {
      inherit pkgs;
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        { services.openssh.enable = recovery; }
      ]
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
  cold = evaluate false true;
  before = evaluate true false;
  after = evaluate true true;
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
