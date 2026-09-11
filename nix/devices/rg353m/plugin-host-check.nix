# Evaluate the shipped normal and rescue configurations, without building ARM
# software or running a VM. Runtime admission remains the generic host's job.
{ pkgs, korri }:
let
  inherit (pkgs) lib;
  publicKey = "korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98=";
  binding = {
    "@korri" = {
      inherit publicKey;
      cacheUrl = "https://github.com/korri-os/plugins/releases/download/cache/";
    };
  };
  check =
    device:
    let
      c = device.config;
      host = c.services.korri.pluginHost;
      restore = c.systemd.services.korri-plugin-host;
      pluginNames = names: builtins.filter (lib.hasPrefix "korri-plugin-") names;
      # These existing native host paths may be created only as empty roots.
      # No receipt, admission snapshot, source list, or GC selection is seeded.
      hostDirectories = builtins.filter (lib.hasInfix "korri-plugin-host") c.systemd.tmpfiles.rules;
      listensOnSsh =
        address:
        builtins.elem address [
          "22"
          "2222"
        ]
        || lib.hasSuffix ":22" address
        || lib.hasSuffix ":2222" address;
    in
    assert c.networking.hostName == "haku";
    assert host.enable;
    assert host.publishers == binding;
    assert builtins.fromJSON c.environment.etc."korri-plugin-host/publishers.json".text == binding;
    assert builtins.elem publicKey c.nix.settings.trusted-public-keys;
    assert host.officialCatalogUrl == null;
    # The plugin cache is selected explicitly by the importer, not installed
    # as a general substitute source for device generations.
    assert c.nix.settings.substituters == [ "https://cache.nixos.org/" ];
    assert c.nix.settings.max-jobs == 0;
    assert c.nix.settings.builders == "";
    assert !c.nix.distributedBuilds;
    assert c.nix.buildMachines == [ ];
    assert !c.nix.settings.fallback;
    assert c.nix.settings.require-sigs;
    assert c.nix.settings.always-allow-substitutes;
    assert builtins.elem "nix-command" c.nix.settings.experimental-features;
    assert !c.services.openssh.enable;
    assert !c.services.openssh.openFirewall;
    assert !(c.systemd.services ? sshd);
    assert !(c.systemd.services ? sshd-keygen);
    assert !(c.systemd.sockets ? sshd);
    assert lib.all (
      socket:
      !(lib.any listensOnSsh (
        socket.listenStreams ++ lib.toList (socket.socketConfig.ListenStream or [ ])
      ))
    ) (lib.attrValues c.systemd.sockets);
    assert c.users.users.root.openssh.authorizedKeys.keys == [ ];
    assert c.users.users.root.openssh.authorizedKeys.keyFiles == [ ];
    # Preserve the shared physical-console policy, not new network enrollment.
    assert c.users.users.root.initialHashedPassword == "";
    assert c.users.users.root.hashedPassword == null;
    assert c.users.users.root.password == null;
    assert c.users.users.root.initialPassword == null;
    assert c.users.users.root.hashedPasswordFile == null;
    assert c.services.openssh.settings.PermitRootLogin == "prohibit-password";
    assert !c.services.openssh.settings.PasswordAuthentication;
    assert !c.services.openssh.settings.KbdInteractiveAuthentication;
    assert c.users.users.sshd.isSystemUser;
    assert c.users.users.sshd.group == "sshd";
    assert c.users.groups ? sshd;
    assert c.security.pam.services.sshd.startSession;
    assert !c.security.pam.services.sshd.unixAuth;
    assert c.security.pam.services.sshd.rules.session.systemd.enable;
    assert builtins.elem "tun" c.boot.kernelModules;
    assert pluginNames (map lib.getName c.environment.systemPackages) == [ "korri-plugin-host" ];
    assert builtins.elem host.package c.environment.systemPackages;
    assert
      pluginNames (builtins.attrNames c.environment.etc) == [
        "korri-plugin-host/publishers.json"
      ];
    assert pluginNames (builtins.attrNames c.systemd.services) == [ "korri-plugin-host" ];
    assert
      hostDirectories == [
        "d /var/lib/korri-plugin-host 0700 root root -"
        "d /run/korri-plugin-host 0755 root root -"
        "d /nix/var/nix/gcroots/korri-plugin-host 0700 root root -"
      ];
    assert restore.enable;
    assert restore.wantedBy == [ "multi-user.target" ];
    assert builtins.elem "korrid.service" restore.before;
    assert lib.all (unit: builtins.elem unit restore.after) [
      "systemd-tmpfiles-setup.service"
      "network.target"
      "firewall.service"
    ];
    assert restore.serviceConfig.ExecStart == "${host.package}/bin/korri-plugin restore-all";
    assert restore.serviceConfig.Type == "oneshot";
    assert restore.serviceConfig.User == "root";
    assert
      restore.restartTriggers == [
        c.environment.etc."korri-plugin-host/publishers.json".source
      ];
    true;
in
assert check korri.nixosConfigurations.rg353m;
assert check korri.nixosConfigurations.rg353m-rescue;
pkgs.runCommand "rg353m-plugin-host-module-check" { } ''
  touch "$out"
''
