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
  ownerBindingFile = ./test-data/public-owner-binding.json;
  ownerBindingStoreFile = pkgs.writeText "korrid-owner-binding.json" (
    builtins.readFile ownerBindingFile
  );
  peerPublicKey = builtins.concatStringsSep "" (lib.replicate 64 "2");
  androidUpstreamsTemplate = ./deploy/upstreams.android.json;
  base = {
    users.groups.korri.gid = 1000;
    users.users.korri = {
      isNormalUser = true;
      uid = 1000;
      group = "korri";
    };
    services.korridLinuxDevice = {
      package = korridPackage;
      uid = 976;
      gid = 976;
      runtimeUser = "korri";
      runtimeUid = 1000;
      runtimeGid = 1000;
      inputdUid = 977;
      controlGid = 977;
      localSignerUid = 978;
      localSignerGid = 978;
      inherit deviceConfig;
      streamPrivateStateRoot = "/home/korri/.config/sunshine";
      certificateControlDirectory = lib.mkDefault "/run/korri-certificate-control";
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
  pluginFree = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      streamPrivateStateRoot = lib.mkForce null;
      certificateControlDirectory = lib.mkForce null;
    };
  };
  pluginFreeService = pluginFree.config.systemd.services.korrid;
  advertised = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      advertisedEndpoints = [
        "https://device.example:443/"
        "http://[::1]:43117"
      ];
      moonlightAddress = "device:47989";
    };
  };
  invalidAdvertisements =
    map
      (
        value:
        evaluate {
          services.korridLinuxDevice.enable = true;
          services.korridLinuxDevice.advertisedEndpoints = [ value ];
        }
      )
      [
        "ws://localhost:43117"
        "https://u@host"
        "http://host/path"
        "http://host?x"
        "http://host#x"
        "http://host:0"
        "http://host:65536"
        "http://host:"
        "http://host//"
        "http://host/../"
        "http://host\\\\x"
        "http://host:abc"
        "http://host "
        "http://[abc]"
        "http://[:::1]"
        "http://[:1::2]"
        "http://[1::2:]"
        "http://[1:2:3:4:5:6:7:8:9]"
      ];
  queryOnlyEmptyMoonlight = evaluate {
    services.korridLinuxDevice.enable = true;
    services.korridLinuxDevice.moonlightAddress = "";
  };
  invalidMoonlight = evaluate {
    services.korridLinuxDevice.enable = true;
    services.korridLinuxDevice.advertisedEndpoints = [ "http://device:43117" ];
    services.korridLinuxDevice.moonlightAddress = "";
  };
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
      runtimeUid = lib.mkForce 976;
    };
  };
  broadRuntime = evaluate {
    services.korridLinuxDevice.enable = true;
    users.users.korri.extraGroups = [ "input" ];
  };
  certificateControlRuntime = evaluate {
    services.korridLinuxDevice.enable = true;
    users.users.korri.extraGroups = [ "korrid" ];
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
  # A socket outside the hidden directory would be reachable by games, which
  # is exactly what hiding that directory prevents.
  compositorSocketOutsideHiddenDirectory = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      compositorControlSocket = "/run/elsewhere/sway-ipc.sock";
    };
  };
  focusExclusionWithoutCompositor = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      neverFocusAppIds = [ "chrome-kiosk" ];
    };
  };
  focusExclusionWithComma = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      compositorControlSocket = "/run/korri-test/compositor-control/sway-ipc.sock";
      neverFocusAppIds = [ "one,two" ];
    };
  };
  focusEnabled = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      compositorControlSocket = "/run/korri-test/compositor-control/sway-ipc.sock";
      neverFocusAppIds = [
        "chrome-kiosk"
        "other-hub"
      ];
    };
  };
  emptyRelays = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      relays = lib.mkForce [ ];
    };
  };
  tooManyRelays = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      relays = lib.mkForce (map (index: "wss://relay-${toString index}.example.com") (lib.range 1 9));
    };
  };
  loopbackRelay = evaluate {
    services.korridLinuxDevice = {
      enable = true;
      relays = lib.mkForce [ "ws://127.0.0.1:7447/" ];
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
  signerService = enabled.config.systemd.services.korri-local-signer;
  signerCredentialService =
    enabled.config.systemd.services.korri-local-signer-device-credential;
  signerSocket = enabled.config.systemd.sockets.korri-local-signer;
  socket = enabled.config.systemd.sockets.korrid-control;
  polkit = enabled.config.security.polkit.extraConfig;
  tmpfiles = enabled.config.systemd.tmpfiles.rules;
  customService = customPaths.config.systemd.services.korrid;
  customSocket = customPaths.config.systemd.sockets.korrid-control;
  customTmpfiles = customPaths.config.systemd.tmpfiles.rules;
  bundledService = bundled.config.systemd.services.korrid;
  bundledSignerService = bundled.config.systemd.services.korri-local-signer;
in
assert allAssertionsPass enabled;
assert allAssertionsPass pluginFree;
assert !(pluginFreeService.environment ? KORRID_STREAM_PRIVATE_STATE_ROOT);
assert !(pluginFreeService.environment ? KORRID_CERTIFICATE_CONTROL_DIRECTORY);
assert
  !(builtins.elem "/home/korri/.config/sunshine" pluginFreeService.serviceConfig.InaccessiblePaths);
assert allAssertionsPass advertised;
assert
  advertised.config.systemd.services.korrid.environment.KORRID_ADVERTISED_ENDPOINTS
  == ''["https://device.example:443/","http://[::1]:43117"]'';
assert
  advertised.config.systemd.services.korrid.environment.KORRID_MOONLIGHT_ADDRESS == "device:47989";
assert !(service.environment ? KORRID_MOONLIGHT_ADDRESS);
assert lib.all (hasFailedAssertion "advertisedEndpoints") invalidAdvertisements;
assert allAssertionsPass queryOnlyEmptyMoonlight;
assert hasFailedAssertion "moonlightAddress" invalidMoonlight;
assert service.serviceConfig.User == "korrid";
assert service.serviceConfig.User != "korri";
assert
  builtins.removeAttrs service.environment [ "PATH" ] == {
    HOSTNAME = enabled.config.networking.hostName;
    KORRID_ADVERTISED_ENDPOINTS = "[]";
    KORRID_ADDRESS = "127.0.0.1:43117";
    KORRID_CERTIFICATE_CONTROL_DIRECTORY = "/run/korri-certificate-control";
    KORRID_COMPOSITOR_CONTROL_DIRECTORY = "/run/korri-compositor";
    KORRID_CONTROL_DIRECTORY = "/run/korrid-control";
    KORRID_CONTROL_PEER_GID = "977";
    KORRID_CONTROL_PEER_UID = "977";
    KORRID_CONTROL_SOCKET = "/run/korrid-control/control.sock";
    KORRID_RUNTIME_GID = "1000";
    KORRID_RUNTIME_UID = "1000";
    KORRID_HOST_CONFIG = toString deviceConfig;
    KORRID_MODE = "host";
    KORRID_PRIVATE_STATE_ROOT = "/var/lib/korrid";
    KORRID_LOCAL_SIGNER_SOCKET = "/run/korri-local-signer/signer.sock";
    KORRID_LOCAL_SIGNER_PUBLIC_KEY_FILE = "/run/korri-local-signer/public/person.pub";
    KORRID_RELAYS = ''["wss://relay.example.com"]'';
    KORRID_STORAGE_ROOT = "/var/lib/korri";
    KORRID_STREAM_PRIVATE_STATE_ROOT = "/home/korri/.config/sunshine";
    KORRID_SYSTEMCTL = "${pkgs.systemd}/bin/systemctl";
    KORRID_SYSTEMD_RUN = "${pkgs.systemd}/bin/systemd-run";
    KORRID_UPSTREAMS = ''[{"baseUrl":"http://zao:43117","devicePublicKey":"${peerPublicKey}","kind":"native","label":"zao","moonlightAddress":"zao:47989"}]'';
  };
assert service.environment.KORRID_RUNTIME_UID == "1000";
assert service.environment.KORRID_RUNTIME_GID == "1000";
assert service.environment.KORRID_CONTROL_PEER_UID == "977";
assert service.environment.KORRID_CONTROL_PEER_GID == "977";
assert service.environment.KORRID_SYSTEMD_RUN == "${pkgs.systemd}/bin/systemd-run";
assert service.environment.KORRID_SYSTEMCTL == "${pkgs.systemd}/bin/systemctl";
assert service.environment.KORRID_ADDRESS == "127.0.0.1:43117";
assert service.environment.KORRID_PRIVATE_STATE_ROOT == "/var/lib/korrid";
assert service.environment.KORRID_LOCAL_SIGNER_SOCKET == "/run/korri-local-signer/signer.sock";
assert service.environment.KORRID_LOCAL_SIGNER_PUBLIC_KEY_FILE == "/run/korri-local-signer/public/person.pub";
assert service.environment.KORRID_STREAM_PRIVATE_STATE_ROOT == "/home/korri/.config/sunshine";
assert service.environment.KORRID_CONTROL_SOCKET == "/run/korrid-control/control.sock";
assert service.environment.KORRID_CONTROL_DIRECTORY == "/run/korrid-control";
assert service.environment.KORRID_COMPOSITOR_CONTROL_DIRECTORY == "/run/korri-compositor";
# Without a socket, runtime Return and running-session restart recovery fail
# closed to Portal ownership. Korrid also stays out of the runtime user's group.
assert !(service.environment ? KORRID_COMPOSITOR_CONTROL_SOCKET);
assert !(service.environment ? KORRID_SWAYMSG);
assert service.serviceConfig.SupplementaryGroups == [ ];
assert
  (focusEnabled.config.systemd.services.korrid.environment.KORRID_COMPOSITOR_CONTROL_SOCKET)
  == "/run/korri-test/compositor-control/sway-ipc.sock";
assert
  (focusEnabled.config.systemd.services.korrid.environment.KORRID_NEVER_FOCUS_APP_IDS)
  == "chrome-kiosk,other-hub";
assert
  (focusEnabled.config.systemd.services.korrid.environment.KORRID_SWAYMSG)
  == "${pkgs.sway}/bin/swaymsg";
assert focusEnabled.config.systemd.services.korrid.serviceConfig.SupplementaryGroups == [ "korri" ];
assert service.environment.KORRID_CERTIFICATE_CONTROL_DIRECTORY == "/run/korri-certificate-control";
assert
  builtins.removeAttrs identityService.environment [ "PATH" ] == {
    KORRID_PRIVATE_STATE_ROOT = "/var/lib/korrid";
  };
assert
  identityService.serviceConfig.ExecStart
  == "${korridPackage}/bin/korrid identity import --file ${ownerBindingStoreFile}";
assert builtins.length identityService.serviceConfig.ExecStartPre == 1;
assert
  let
    validator = builtins.elemAt identityService.serviceConfig.ExecStartPre 0;
    validatorText = builtins.readFile validator;
  in
  lib.hasInfix "korrid-validate-owner-binding" validator
  && lib.hasInfix (builtins.unsafeDiscardStringContext (toString ownerBindingStoreFile)) validatorText
  && !(lib.hasInfix (builtins.unsafeDiscardStringContext (toString ownerBindingFile)) validatorText)
  && lib.hasInfix ''[ ! -f "$binding" ] || [ -L "$binding" ]'' validatorText;
assert identityService.serviceConfig.StateDirectory == "korrid";
assert identityService.serviceConfig.StateDirectoryMode == "0700";
assert identityService.serviceConfig.UMask == "0077";
assert identityService.serviceConfig.RestrictAddressFamilies == [ "AF_UNIX" ];
assert identityService.serviceConfig.ReadWritePaths == [ "/var/lib/korrid" ];
assert builtins.elem "/var/lib/korri" identityService.serviceConfig.InaccessiblePaths;
assert builtins.elem "-/home/korri/.config/sunshine"
  identityService.serviceConfig.InaccessiblePaths;
assert builtins.elem "-/run/korri-compositor" identityService.serviceConfig.InaccessiblePaths;
assert builtins.elem "-/run/korri-certificate-control"
  identityService.serviceConfig.InaccessiblePaths;
assert builtins.elem "/dev/uinput" identityService.serviceConfig.InaccessiblePaths;
assert builtins.elem "korrid-control.socket" service.requires;
assert builtins.elem "korrid-identity.service" service.requires;
assert builtins.elem "korrid-identity.service" service.after;
assert builtins.elem "korri-local-signer.service" service.requires;
assert builtins.elem "korri-local-signer.service" service.after;
assert signerService.serviceConfig.User == "korri-local-signer";
assert signerService.serviceConfig.Group == "korri-local-signer";
assert signerService.environment.KORRI_LOCAL_SIGNER_PRIVATE_STATE_ROOT == "/var/lib/korri-local-signer";
assert signerService.environment.KORRI_LOCAL_SIGNER_PEER_UID == "976";
assert signerService.environment.KORRI_LOCAL_SIGNER_PEER_GID == "976";
assert signerService.environment.KORRI_LOCAL_SIGNER_PUBLIC_KEY_FILE == "/run/korri-local-signer/public/person.pub";
assert builtins.elem "korri-local-signer-device-credential.service" signerService.requires;
assert builtins.elem "korri-local-signer-device-credential.service" signerService.after;
assert signerService.serviceConfig.LoadCredential == "expected-device-public-key:/run/korri-local-signer/expected-device-public-key";
assert signerService.serviceConfig.ExecStart == "${korridPackage}/bin/korri-local-signer";
assert signerService.serviceConfig.StateDirectory == "korri-local-signer";
assert signerService.serviceConfig.StateDirectoryMode == "0700";
assert signerService.serviceConfig.RestrictAddressFamilies == [ "AF_UNIX" ];
assert signerService.serviceConfig.PrivateNetwork;
assert signerService.serviceConfig.ReadWritePaths == [
  "/var/lib/korri-local-signer"
  "/run/korri-local-signer/public"
];
assert signerCredentialService.serviceConfig.User == "root";
assert signerCredentialService.serviceConfig.ReadOnlyPaths == [ "/var/lib/korrid" ];
assert signerCredentialService.serviceConfig.ReadWritePaths == [ "/run/korri-local-signer" ];
assert builtins.elem "/var/lib/korri-local-signer" signerCredentialService.serviceConfig.InaccessiblePaths;
assert builtins.elem "/var/lib/korrid" signerService.serviceConfig.InaccessiblePaths;
assert builtins.elem "/var/lib/korri-local-signer" service.serviceConfig.InaccessiblePaths;
assert signerSocket.socketConfig.ListenStream == "/run/korri-local-signer/signer.sock";
assert signerSocket.socketConfig.SocketUser == "korri-local-signer";
assert signerSocket.socketConfig.SocketGroup == "korrid";
assert signerSocket.socketConfig.SocketMode == "0660";
assert signerSocket.socketConfig.Service == "korri-local-signer.service";
assert socket.socketConfig.ListenStream == "/run/korrid-control/control.sock";
assert socket.socketConfig.SocketUser == "root";
assert socket.socketConfig.SocketGroup == "korri-control";
assert socket.socketConfig.SocketMode == "0660";
assert socket.socketConfig.Service == "korrid.service";
assert builtins.elem "systemd-tmpfiles-setup.service" socket.requires;
assert builtins.elem "systemd-tmpfiles-setup.service" socket.after;
assert builtins.elem "systemd-tmpfiles-resetup.service" socket.after;
assert builtins.elem "d /run/korrid-control 0750 root korri-control -" tmpfiles;
assert builtins.elem "d /run/korri-local-signer/public 2750 korri-local-signer korrid -" tmpfiles;
assert builtins.elem "d /var/lib/korrid 0700 korrid korrid -" tmpfiles;
assert builtins.elem "d /var/lib/korrid/identity 0700 korrid korrid -" tmpfiles;
assert builtins.elem "d /run/korri-local-signer 0751 root korrid -" tmpfiles;
assert builtins.elem "d /var/lib/korri-local-signer 0700 korri-local-signer korri-local-signer -" tmpfiles;
assert builtins.elem "d /var/lib/korri-local-signer/identity 0700 korri-local-signer korri-local-signer -" tmpfiles;
assert builtins.elem "d /dev/inputplumber 0700 root root -" tmpfiles;
assert builtins.elem "d /dev/inputplumber/sources 0700 root root -" tmpfiles;
assert builtins.elem "systemd-tmpfiles-setup-dev.service" service.after;
assert builtins.elem "systemd-tmpfiles-resetup.service" service.after;
assert builtins.elem "korri-input-source-guard.service" service.after;
assert builtins.elem "-/dev/inputplumber/sources" service.serviceConfig.InaccessiblePaths;
assert builtins.elem "/home/korri/.config/sunshine" service.serviceConfig.InaccessiblePaths;
assert enabled.config.users.groups.korri-control.gid == 977;
assert !(builtins.elem "korri-control" enabled.config.users.users.korri.extraGroups);
assert !(builtins.elem "korrid" enabled.config.users.users.korri.extraGroups);
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
assert allAssertionsPass loopbackRelay;
assert
  loopbackRelay.config.systemd.services.korrid.environment.KORRID_RELAYS
  == ''["ws://127.0.0.1:7447"]'';
assert builtins.elem "korri-bundle-selector.service" bundledService.requires;
assert builtins.elem "korri-bundle-selector.service" bundledService.after;
assert builtins.elem "korri-bundle-selector.service" bundled.config.systemd.services.korrid-identity.requires;
assert builtins.elem "korri-bundle-selector.service" bundled.config.systemd.services.korrid-identity.after;
assert bundledService.environment.KORRI_BUNDLE_ACTIVE == "/nix/var/nix/gcroots/korri-bundle/active";
assert
  bundled.config.systemd.services.korrid-identity.environment.KORRI_BUNDLE_ACTIVE
  == "/nix/var/nix/gcroots/korri-bundle/active";
assert bundledService.serviceConfig.ExecStart == "${inputdPackage}/bin/korri-bundle-launch korrid";
assert
  bundledSignerService.serviceConfig.ExecStart
  == "${inputdPackage}/bin/korri-bundle-launch local-signer";
assert builtins.elem "korri-bundle-selector.service" bundledSignerService.requires;
assert bundledSignerService.environment.KORRI_BUNDLE_ACTIVE == "/nix/var/nix/gcroots/korri-bundle/active";
assert
  bundled.config.systemd.services.korrid-identity.serviceConfig.ExecStart
  == "${inputdPackage}/bin/korri-bundle-launch korrid identity import --file ${ownerBindingStoreFile}";
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
assert hasFailedAssertion "service UIDs must be distinct" sameUid;
assert hasFailedAssertion "runtime user must not hold raw input" broadRuntime;
assert hasFailedAssertion "korrid, or local-signer service groups" certificateControlRuntime;
assert hasFailedAssertion
  "korrid and local-signer private state roots and optional stream-host private state root must be normalized absolute paths"
  invalidPrivatePath;
assert hasFailedAssertion "controlSocket and its directory must be normalized absolute paths"
  invalidControlPath;
assert hasFailedAssertion "compositorControlDirectory must be a normalized absolute path"
  invalidCompositorControlPath;
assert hasFailedAssertion "certificateControlDirectory must be null or a normalized absolute path"
  invalidCertificateControlPath;
assert hasFailedAssertion "one to eight unique normalized" emptyRelays;
assert hasFailedAssertion "one to eight unique normalized" tooManyRelays;
assert hasFailedAssertion "one to eight unique normalized" insecureRelay;
assert hasFailedAssertion "one to eight unique normalized" duplicateNormalizedRelays;
assert hasFailedAssertion "native peers require" invalidPeerKey;
assert hasFailedAssertion "native peers require" missingMoonlightAddress;
assert hasFailedAssertion "exact Korri korrid package" wrongPackage;
assert hasFailedAssertion "no Nostr secret-key form" secretOwnerBinding;
assert hasFailedAssertion "no Nostr secret-key form" secretFieldOwnerBinding;
assert hasFailedAssertion "must sit inside compositorControlDirectory"
  compositorSocketOutsideHiddenDirectory;
assert hasFailedAssertion "needs compositorControlSocket" focusExclusionWithoutCompositor;
assert hasFailedAssertion "must not contain a comma" focusExclusionWithComma;
assert evaluationRejected sameUid;
assert evaluationRejected broadRuntime;
assert evaluationRejected certificateControlRuntime;
assert evaluationRejected invalidPrivatePath;
assert evaluationRejected invalidControlPath;
assert evaluationRejected invalidCompositorControlPath;
assert evaluationRejected invalidCertificateControlPath;
assert evaluationRejected emptyRelays;
assert evaluationRejected tooManyRelays;
assert evaluationRejected insecureRelay;
assert evaluationRejected duplicateNormalizedRelays;
assert evaluationRejected invalidPeerKey;
assert evaluationRejected missingMoonlightAddress;
assert evaluationRejected wrongPackage;
assert evaluationRejected secretOwnerBinding;
assert evaluationRejected secretFieldOwnerBinding;
pkgs.runCommand "korrid-linux-device-module-check" { nativeBuildInputs = [ pkgs.jq ]; } ''
  jq -e '
    length == 1
    and .[0].label == "zao"
    and .[0].kind == "native"
    and .[0].baseUrl == "http://zao:43117"
    and .[0].moonlightAddress == "zao:47989"
    and .[0].devicePublicKey == "__ZAO_DEVICE_PUBLIC_KEY_FROM_IDENTITY_STATUS__"
  ' ${androidUpstreamsTemplate} >/dev/null
  touch "$out"
''
