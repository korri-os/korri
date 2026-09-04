{
  pkgs,
  module,
  bundleModule,
  korridPackage,
  inputdPackage,
  korriBundle,
}:
let
  lib = pkgs.lib;
  deviceConfig = pkgs.writeText "korrid-host.toml" ''
    label = "test"
  '';
  ownerBindingFile = pkgs.writeText "korrid-owner-binding.json" ''
    {"id":"public-binding","pubkey":"${builtins.concatStringsSep "" (lib.replicate 64 "1")}"}
  '';
  peerPublicKey = builtins.concatStringsSep "" (lib.replicate 64 "2");
  base = {
    users.groups.games.gid = 1001;
    users.users.gameplay = {
      isNormalUser = true;
      uid = 1001;
      group = "games";
    };
    services.korridLinuxDevice = {
      package = korridPackage;
      uid = 976;
      gid = 976;
      gameplayUser = "gameplay";
      gameplayUid = 1001;
      gameplayGid = 1001;
      inputdUid = 977;
      controlGid = 977;
      inherit deviceConfig;
      sunshinePrivateStateRoot = "/home/gameplay/.config/sunshine";
      relays = [ "wss://relay.example.com/" ];
      nativePeers = [
        {
          label = "zao";
          baseUrl = "http://zao:43117";
          devicePublicKey = peerPublicKey;
          moonlightAddress = "zao:47989";
        }
      ];
      inherit ownerBindingFile;
    };
  };
  evaluate =
    extra:
    import "${pkgs.path}/nixos/lib/eval-config.nix" {
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        bundleModule
        module
        base
        {
          system.stateVersion = "26.05";
          boot.loader.grub.enable = false;
          fileSystems."/" = {
            device = "none";
            fsType = "tmpfs";
          };
        }
        extra
      ];
    };
  enabled = evaluate { services.korridLinuxDevice.enable = true; };
  bundled = evaluate {
    services.korriBundle = {
      enable = true;
      initialPackage = korriBundle;
      launcherPackage = inputdPackage;
    };
    services.korridLinuxDevice.enable = true;
  };
  customPaths = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      privateStateRoot = "/srv/korri-test/recovery";
      controlSocket = "/run/korri-test/control/device.sock";
      compositorControlDirectory = "/run/korri-test/compositor-control";
      certificateControlDirectory = "/run/korri-test/certificate-control";
    };
  };
  sameUid = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      gameplayUid = lib.mkForce 976;
    };
  };
  broadGame = evaluate {
    services.korridLinuxDevice.enable = true;
    users.users.gameplay.extraGroups = [ "input" ];
  };
  certificateControlGame = evaluate {
    services.korridLinuxDevice.enable = true;
    users.users.gameplay.extraGroups = [ "korrid" ];
  };
  invalidPrivatePath = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      privateStateRoot = "/srv/korrid/../recovery";
    };
  };
  invalidControlPath = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      controlSocket = "relative/control.sock";
    };
  };
  invalidCertificateControlPath = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      certificateControlDirectory = "relative/certificate-control";
    };
  };
  invalidCompositorControlPath = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      compositorControlDirectory = "relative/compositor-control";
    };
  };
  emptyRelays = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      relays = lib.mkForce [ ];
    };
  };
  insecureRelay = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      relays = lib.mkForce [ "ws://relay.example.com" ];
    };
  };
  duplicateNormalizedRelays = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      relays = lib.mkForce [
        "wss://relay.example.com"
        "wss://relay.example.com/"
      ];
    };
  };
  invalidPeerKey = evaluate {
    services.korridLinuxDevice.enable = true;
    services.korridLinuxDevice.nativePeers = lib.mkForce [
      {
        label = "zao";
        baseUrl = "http://zao:43117";
        devicePublicKey = "not-a-key";
        moonlightAddress = "zao:47989";
      }
    ];
  };
  missingMoonlightAddress = evaluate {
    services.korridLinuxDevice.enable = true;
    services.korridLinuxDevice.nativePeers = lib.mkForce [
      {
        label = "zao";
        baseUrl = "http://zao:43117";
        devicePublicKey = peerPublicKey;
        moonlightAddress = "";
      }
    ];
  };
  wrongPackage = evaluate {
    services.korridLinuxDevice.enable = true;
    services.korridLinuxDevice.package = lib.mkForce pkgs.hello;
  };
  secretOwnerBinding = evaluate {
    services.korridLinuxDevice.enable = true;
    services.korridLinuxDevice.ownerBindingFile = lib.mkForce (
      pkgs.writeText "secret-owner-binding.json" ''{"content":"nsec1must-not-enter-the-store-option"}''
    );
  };
  secretFieldOwnerBinding = evaluate {
    services.korridLinuxDevice.enable = true;
    services.korridLinuxDevice.ownerBindingFile = lib.mkForce (
      pkgs.writeText "secret-field-owner-binding.json" ''{"privateKey":"must-not-enter"}''
    );
  };
  allAssertionsPass = system: lib.all (entry: entry.assertion) system.config.assertions;
  hasFailedAssertion =
    needle: system:
    lib.any (entry: !entry.assertion && lib.hasInfix needle entry.message) system.config.assertions;
  evaluationRejected =
    system: !(builtins.tryEval system.config.system.build.toplevel.drvPath).success;
  service = enabled.config.systemd.services.korrid;
  identityService = enabled.config.systemd.services.korrid-identity;
  socket = enabled.config.systemd.sockets.korrid-control;
  polkit = enabled.config.security.polkit.extraConfig;
  tmpfiles = enabled.config.systemd.tmpfiles.rules;
  customService = customPaths.config.systemd.services.korrid;
  customSocket = customPaths.config.systemd.sockets.korrid-control;
  customTmpfiles = customPaths.config.systemd.tmpfiles.rules;
  bundledService = bundled.config.systemd.services.korrid;
in
assert allAssertionsPass enabled;
assert service.serviceConfig.User == "korrid";
assert service.serviceConfig.User != "gameplay";
assert
  builtins.removeAttrs service.environment [ "PATH" ] == {
    KORRID_ADDRESS = "127.0.0.1:43117";
    KORRID_CERTIFICATE_CONTROL_DIRECTORY = "/run/korri-certificate-control";
    KORRID_COMPOSITOR_CONTROL_DIRECTORY = "/run/korri-compositor";
    KORRID_CONTROL_DIRECTORY = "/run/korrid-control";
    KORRID_CONTROL_PEER_GID = "977";
    KORRID_CONTROL_PEER_UID = "977";
    KORRID_CONTROL_SOCKET = "/run/korrid-control/control.sock";
    KORRID_GAMEPLAY_GID = "1001";
    KORRID_GAMEPLAY_UID = "1001";
    KORRID_HOST_CONFIG = toString deviceConfig;
    KORRID_MODE = "host";
    KORRID_PRIVATE_STATE_ROOT = "/var/lib/korrid";
    KORRID_RELAYS = ''["wss://relay.example.com"]'';
    KORRID_STORAGE_ROOT = "/var/lib/korri";
    KORRID_SUNSHINE_PRIVATE_STATE_ROOT = "/home/gameplay/.config/sunshine";
    KORRID_SYSTEMCTL = "${pkgs.systemd}/bin/systemctl";
    KORRID_SYSTEMD_RUN = "${pkgs.systemd}/bin/systemd-run";
    KORRID_UPSTREAMS = ''[{"baseUrl":"http://zao:43117","devicePublicKey":"${peerPublicKey}","kind":"native","label":"zao","moonlightAddress":"zao:47989"}]'';
  };
assert service.environment.KORRID_GAMEPLAY_UID == "1001";
assert service.environment.KORRID_GAMEPLAY_GID == "1001";
assert service.environment.KORRID_CONTROL_PEER_UID == "977";
assert service.environment.KORRID_CONTROL_PEER_GID == "977";
assert service.environment.KORRID_SYSTEMD_RUN == "${pkgs.systemd}/bin/systemd-run";
assert service.environment.KORRID_SYSTEMCTL == "${pkgs.systemd}/bin/systemctl";
assert service.environment.KORRID_ADDRESS == "127.0.0.1:43117";
assert service.environment.KORRID_PRIVATE_STATE_ROOT == "/var/lib/korrid";
assert service.environment.KORRID_SUNSHINE_PRIVATE_STATE_ROOT == "/home/gameplay/.config/sunshine";
assert service.environment.KORRID_CONTROL_SOCKET == "/run/korrid-control/control.sock";
assert service.environment.KORRID_CONTROL_DIRECTORY == "/run/korrid-control";
assert service.environment.KORRID_COMPOSITOR_CONTROL_DIRECTORY == "/run/korri-compositor";
assert service.environment.KORRID_CERTIFICATE_CONTROL_DIRECTORY == "/run/korri-certificate-control";
assert
  builtins.removeAttrs identityService.environment [ "PATH" ] == {
    KORRID_PRIVATE_STATE_ROOT = "/var/lib/korrid";
  };
assert
  identityService.serviceConfig.ExecStart
  == "${korridPackage}/bin/korrid identity import --file ${ownerBindingFile}";
assert builtins.length identityService.serviceConfig.ExecStartPre == 1;
assert
  let
    validator = builtins.elemAt identityService.serviceConfig.ExecStartPre 0;
  in
  lib.hasInfix "korrid-validate-owner-binding" validator
  && lib.hasInfix ''[ ! -f "$binding" ] || [ -L "$binding" ]'' (builtins.readFile validator);
assert identityService.serviceConfig.StateDirectory == "korrid";
assert identityService.serviceConfig.StateDirectoryMode == "0700";
assert identityService.serviceConfig.UMask == "0077";
assert identityService.serviceConfig.RestrictAddressFamilies == [ "AF_UNIX" ];
assert identityService.serviceConfig.ReadWritePaths == [ "/var/lib/korrid" ];
assert builtins.elem "/var/lib/korri" identityService.serviceConfig.InaccessiblePaths;
assert builtins.elem "/home/gameplay/.config/sunshine"
  identityService.serviceConfig.InaccessiblePaths;
assert builtins.elem "/dev/uinput" identityService.serviceConfig.InaccessiblePaths;
assert builtins.elem "korrid-control.socket" service.requires;
assert builtins.elem "korrid-identity.service" service.requires;
assert builtins.elem "korrid-identity.service" service.after;
assert socket.socketConfig.ListenStream == "/run/korrid-control/control.sock";
assert socket.socketConfig.SocketUser == "root";
assert socket.socketConfig.SocketGroup == "korri-control";
assert socket.socketConfig.SocketMode == "0660";
assert socket.socketConfig.Service == "korrid.service";
assert builtins.elem "systemd-tmpfiles-setup.service" socket.requires;
assert builtins.elem "systemd-tmpfiles-setup.service" socket.after;
assert builtins.elem "systemd-tmpfiles-resetup.service" socket.after;
assert builtins.elem "d /run/korrid-control 0750 root korri-control -" tmpfiles;
assert builtins.elem "d /var/lib/korrid 0700 korrid korrid -" tmpfiles;
assert builtins.elem "d /var/lib/korrid/identity 0700 korrid korrid -" tmpfiles;
assert builtins.elem "d /dev/inputplumber 0700 root root -" tmpfiles;
assert builtins.elem "d /dev/inputplumber/sources 0700 root root -" tmpfiles;
assert builtins.elem "systemd-tmpfiles-setup-dev.service" service.after;
assert builtins.elem "systemd-tmpfiles-resetup.service" service.after;
assert builtins.elem "korri-input-source-guard.service" service.after;
assert builtins.elem "-/dev/inputplumber/sources" service.serviceConfig.InaccessiblePaths;
assert builtins.elem "/home/gameplay/.config/sunshine" service.serviceConfig.InaccessiblePaths;
assert enabled.config.users.groups.korri-control.gid == 977;
assert !(builtins.elem "korri-control" enabled.config.users.users.gameplay.extraGroups);
assert !(builtins.elem "korrid" enabled.config.users.users.gameplay.extraGroups);
assert builtins.elem "AF_UNIX" service.serviceConfig.RestrictAddressFamilies;
assert builtins.elem "AF_INET" service.serviceConfig.RestrictAddressFamilies;
assert service.serviceConfig.ProtectProc == "invisible";
assert service.serviceConfig.ProcSubset == "pid";
assert lib.hasInfix "action.id == \"org.freedesktop.systemd1.manage-units\"" polkit;
assert lib.hasInfix "subject.user == \"korrid\"" polkit;
assert !(lib.hasInfix "subject.system_unit" polkit);
assert lib.hasInfix ''/^korri-game-[0-9a-f]{32}\.service$/'' polkit;
assert !(lib.hasInfix ''/^korri-game-[0-9a-f]{32}\\.service$/'' polkit);
assert allAssertionsPass bundled;
assert builtins.elem "korri-bundle-selector.service" bundledService.requires;
assert builtins.elem "korri-bundle-selector.service" bundledService.after;
assert bundledService.environment.KORRI_BUNDLE_ACTIVE == "/nix/var/nix/gcroots/korri-bundle/active";
assert
  bundled.config.systemd.services.korrid-identity.environment.KORRI_BUNDLE_ACTIVE
  == "/nix/var/nix/gcroots/korri-bundle/active";
assert bundledService.serviceConfig.ExecStart == "${inputdPackage}/bin/korri-bundle-launch korrid";
assert
  bundled.config.systemd.services.korrid-identity.serviceConfig.ExecStart
  == "${inputdPackage}/bin/korri-bundle-launch korrid identity import --file ${ownerBindingFile}";
assert allAssertionsPass customPaths;
assert customSocket.socketConfig.ListenStream == "/run/korri-test/control/device.sock";
assert customService.environment.KORRID_PRIVATE_STATE_ROOT == "/srv/korri-test/recovery";
assert customService.environment.KORRID_CONTROL_SOCKET == "/run/korri-test/control/device.sock";
assert customService.environment.KORRID_CONTROL_DIRECTORY == "/run/korri-test/control";
assert
  customService.environment.KORRID_COMPOSITOR_CONTROL_DIRECTORY
  == "/run/korri-test/compositor-control";
assert
  customService.environment.KORRID_CERTIFICATE_CONTROL_DIRECTORY
  == "/run/korri-test/certificate-control";
assert builtins.elem "d /run/korri-test/control 0750 root korri-control -" customTmpfiles;
assert hasFailedAssertion "service UID must differ" sameUid;
assert hasFailedAssertion "gameplay user must not hold raw input" broadGame;
assert hasFailedAssertion "korrid service groups" certificateControlGame;
assert hasFailedAssertion
  "privateStateRoot and sunshinePrivateStateRoot must be normalized absolute paths"
  invalidPrivatePath;
assert hasFailedAssertion "controlSocket and its directory must be normalized absolute paths"
  invalidControlPath;
assert hasFailedAssertion "compositorControlDirectory must be a normalized absolute path"
  invalidCompositorControlPath;
assert hasFailedAssertion "certificateControlDirectory must be a normalized absolute path"
  invalidCertificateControlPath;
assert hasFailedAssertion "one to eight unique normalized" emptyRelays;
assert hasFailedAssertion "one to eight unique normalized" insecureRelay;
assert hasFailedAssertion "one to eight unique normalized" duplicateNormalizedRelays;
assert hasFailedAssertion "native peers require" invalidPeerKey;
assert hasFailedAssertion "native peers require" missingMoonlightAddress;
assert hasFailedAssertion "exact Korri korrid package" wrongPackage;
assert hasFailedAssertion "no Nostr secret-key form" secretOwnerBinding;
assert hasFailedAssertion "no Nostr secret-key form" secretFieldOwnerBinding;
assert evaluationRejected sameUid;
assert evaluationRejected broadGame;
assert evaluationRejected certificateControlGame;
assert evaluationRejected invalidPrivatePath;
assert evaluationRejected invalidControlPath;
assert evaluationRejected invalidCompositorControlPath;
assert evaluationRejected invalidCertificateControlPath;
assert evaluationRejected emptyRelays;
assert evaluationRejected insecureRelay;
assert evaluationRejected duplicateNormalizedRelays;
assert evaluationRejected invalidPeerKey;
assert evaluationRejected missingMoonlightAddress;
assert evaluationRejected wrongPackage;
assert evaluationRejected secretOwnerBinding;
assert evaluationRejected secretFieldOwnerBinding;
pkgs.runCommand "korrid-linux-device-module-check" { } ''
  touch "$out"
''
