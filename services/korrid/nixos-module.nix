{ korri }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.korridLinuxDevice;
  bundleCfg =
    config.services.korriBundle or {
      enable = false;
      activePath = "";
      launcherPackage = null;
    };
  system = pkgs.stdenv.hostPlatform.system;
  serviceUser = "korrid";
  serviceGroup = "korrid";
  controlGroup = "korri-control";
  controlDirectory = builtins.dirOf cfg.controlSocket;
  normalizeRelayUrl =
    value:
    let
      originWithSlash = builtins.match "^(wss?://[^/]+)/$" value;
    in
    if originWithSlash == null then value else builtins.elemAt originWithSlash 0;
  relayUrls = map normalizeRelayUrl cfg.relays;
  validRelayUrl =
    value:
    let
      normalized = normalizeRelayUrl value;
      secure =
        builtins.match "^wss://[^[:space:]#/?]+(:[1-9][0-9]*)?(/[^[:space:]#]*)?([?][^[:space:]#]*)?$" normalized
        != null;
      loopbackOrigin =
        origin:
        lib.hasPrefix origin normalized
        &&
          builtins.match "^(:[1-9][0-9]*)?(/[^[:space:]#]*)?([?][^[:space:]#]*)?$" (
            lib.removePrefix origin normalized
          ) != null;
      loopback =
        builtins.match "^ws://[^[:space:]#]+$" normalized != null
        && lib.any loopbackOrigin [
          "ws://localhost"
          "ws://127.0.0.1"
          "ws://[::1]"
        ];
    in
    secure || loopback;
  nativePeersJson = builtins.toJSON (
    map (peer: {
      inherit (peer)
        label
        baseUrl
        devicePublicKey
        moonlightAddress
        ;
      kind = "native";
    }) cfg.nativePeers
  );
  relayJson = builtins.toJSON relayUrls;
  identityExecutable =
    if bundleCfg.enable then
      "${bundleCfg.launcherPackage}/bin/korri-bundle-launch korrid"
    else
      lib.getExe cfg.package;
  ownerBindingRead =
    if cfg.ownerBindingFile == null then
      {
        success = true;
        value = null;
      }
    else
      builtins.tryEval (builtins.readFile cfg.ownerBindingFile);
  ownerBindingStoreFile =
    if cfg.ownerBindingFile == null then
      null
    else
      pkgs.writeText "korrid-owner-binding.json" (
        if ownerBindingRead.success then ownerBindingRead.value else ""
      );
  ownerBindingJson =
    if ownerBindingRead.success && ownerBindingRead.value != null then
      builtins.tryEval (builtins.fromJSON ownerBindingRead.value)
    else
      {
        success = false;
        value = null;
      };
  secretFieldNames = [
    "ncryptsec"
    "nsec"
    "personprivatekey"
    "person_private_key"
    "privatekey"
    "private_key"
    "secret"
    "secretkey"
    "secret_key"
  ];
  containsSecretField =
    value:
    if builtins.isAttrs value then
      lib.any (
        name: builtins.elem (lib.toLower name) secretFieldNames || containsSecretField value.${name}
      ) (builtins.attrNames value)
    else if builtins.isList value then
      lib.any containsSecretField value
    else
      false;
  ownerBindingTextIsPublic =
    ownerBindingRead.success
    && ownerBindingRead.value != null
    && !(lib.hasInfix "nsec1" (lib.toLower ownerBindingRead.value))
    && !(lib.hasInfix "ncryptsec1" (lib.toLower ownerBindingRead.value));
  ownerBindingValidator = pkgs.writeShellScript "korrid-validate-owner-binding" ''
    set -eu
    binding=${lib.escapeShellArg (toString ownerBindingStoreFile)}
    if [ ! -f "$binding" ] || [ -L "$binding" ]; then
      echo 'owner binding must be a regular Nix-store file' >&2
      exit 1
    fi
  '';
  validAbsolutePath =
    path:
    lib.hasPrefix "/" path
    && path != "/"
    && !(lib.hasInfix "//" path)
    && !(lib.hasInfix "/./" path)
    && !(lib.hasSuffix "/." path)
    && !(lib.hasInfix "/../" path)
    && !(lib.hasSuffix "/.." path)
    && builtins.match ".*[[:space:]].*" path == null;
in
{
  options.services.korridLinuxDevice = {
    enable = lib.mkEnableOption "Linux korrid device service";
    package = lib.mkOption {
      type = lib.types.package;
      default = korri.packages.${system}.korrid;
      defaultText = lib.literalExpression "korri.packages.${system}.korrid";
    };
    uid = lib.mkOption { type = lib.types.ints.positive; };
    gid = lib.mkOption { type = lib.types.ints.positive; };
    gameplayUser = lib.mkOption { type = lib.types.str; };
    gameplayUid = lib.mkOption { type = lib.types.ints.positive; };
    gameplayGid = lib.mkOption { type = lib.types.ints.positive; };
    inputdUid = lib.mkOption { type = lib.types.ints.positive; };
    controlGid = lib.mkOption { type = lib.types.ints.positive; };
    address = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1:43117";
    };
    deviceConfig = lib.mkOption {
      type = lib.types.path;
      description = "Root-owned immutable Linux device TOML configuration.";
    };
    storageRoot = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/korri";
    };
    privateStateRoot = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/korrid";
    };
    sunshinePrivateStateRoot = lib.mkOption {
      type = lib.types.str;
      description = "Exact Sunshine private configuration directory hidden from every game unit.";
    };
    compositorControlDirectory = lib.mkOption {
      type = lib.types.str;
      default = "/run/korri-compositor";
      description = "Exact compositor control directory hidden from every game unit.";
    };
    certificateControlDirectory = lib.mkOption {
      type = lib.types.str;
      default = "/run/korri-certificate-control";
      description = "Exact Sunshine certificate-control directory hidden from every game unit.";
    };
    controlSocket = lib.mkOption {
      type = lib.types.str;
      default = "/run/korrid-control/control.sock";
    };
    relays = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      description = "Ordered relay URLs. Production relays use wss://; ws:// is limited to loopback tests.";
    };
    nativePeers = lib.mkOption {
      default = [ ];
      description = "Native peer endpoints serialized to the established KORRID_UPSTREAMS schema.";
      type = lib.types.listOf (
        lib.types.submodule {
          options = {
            label = lib.mkOption { type = lib.types.strMatching "^[A-Za-z0-9._-]+$"; };
            baseUrl = lib.mkOption { type = lib.types.str; };
            devicePublicKey = lib.mkOption { type = lib.types.str; };
            moonlightAddress = lib.mkOption { type = lib.types.str; };
          };
        }
      );
    };
    ownerBindingFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Optional public pre-signed NIP-78 owner binding imported before korrid opens its network listener.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.uid != cfg.gameplayUid;
        message = "korrid's service UID must differ from the untrusted gameplay UID.";
      }
      {
        assertion =
          let
            user = config.users.users.${cfg.gameplayUser} or { };
            group = config.users.groups.${user.group or ""} or { };
          in
          (user.uid or null) == cfg.gameplayUid && (group.gid or null) == cfg.gameplayGid;
        message = "configured gameplay UID/GID must exactly match the gameplay user's primary identity.";
      }
      {
        assertion = cfg.gid != cfg.gameplayGid && cfg.controlGid != cfg.gameplayGid;
        message = "korrid and local-control GIDs must differ from the gameplay GID.";
      }
      {
        assertion =
          lib.getName cfg.package == "korrid"
          && (cfg.package.drvPath or null) == korri.packages.${system}.korrid.drvPath
          && (cfg.package.outPath or null) == korri.packages.${system}.korrid.outPath;
        message = "korridLinuxDevice must use the exact Korri korrid package and provenance.";
      }
      {
        assertion = lib.hasPrefix "/nix/store/" (toString cfg.deviceConfig);
        message = "korrid deviceConfig must be an immutable Nix-store path.";
      }
      {
        assertion =
          builtins.length relayUrls >= 1
          && builtins.length relayUrls <= 8
          && lib.all validRelayUrl relayUrls
          && builtins.length (lib.unique relayUrls) == builtins.length relayUrls;
        message = "korrid relays must contain one to eight unique normalized wss:// URLs, with ws:// allowed only for loopback tests.";
      }
      {
        assertion =
          let
            labels = map (peer: peer.label) cfg.nativePeers;
          in
          builtins.length labels == builtins.length (lib.unique labels)
          && lib.all (
            peer:
            builtins.match "^https?://[^[:space:]#]+$" peer.baseUrl != null
            && builtins.match "^[0-9a-f]{64}$" peer.devicePublicKey != null
            && builtins.match "^[^[:space:]]+$" peer.moonlightAddress != null
          ) cfg.nativePeers;
        message = "native peers require unique labels, HTTP(S) baseUrl values, lowercase 32-byte devicePublicKey values, and explicit moonlightAddress values.";
      }
      {
        assertion =
          cfg.ownerBindingFile == null
          || (
            lib.hasPrefix "/nix/store/" (toString cfg.ownerBindingFile)
            && ownerBindingRead.success
            && ownerBindingJson.success
            && ownerBindingTextIsPublic
            && !containsSecretField ownerBindingJson.value
          );
        message = "ownerBindingFile must be a regular public JSON file in the Nix store with no Nostr secret-key form or JSON secret field.";
      }
      {
        assertion =
          validAbsolutePath cfg.privateStateRoot && validAbsolutePath cfg.sunshinePrivateStateRoot;
        message = "korrid privateStateRoot and sunshinePrivateStateRoot must be normalized absolute paths.";
      }
      {
        assertion = validAbsolutePath cfg.controlSocket && validAbsolutePath controlDirectory;
        message = "korrid controlSocket and its directory must be normalized absolute paths.";
      }
      {
        assertion = validAbsolutePath cfg.compositorControlDirectory;
        message = "korrid compositorControlDirectory must be a normalized absolute path.";
      }
      {
        assertion = validAbsolutePath cfg.certificateControlDirectory;
        message = "korrid certificateControlDirectory must be a normalized absolute path.";
      }
      {
        assertion =
          let
            user = config.users.users.${cfg.gameplayUser} or { };
          in
          !(builtins.elem "input" (user.extraGroups or [ ]))
          && !(builtins.elem "uinput" (user.extraGroups or [ ]))
          && !(builtins.elem controlGroup (user.extraGroups or [ ]))
          && !(builtins.elem serviceGroup (user.extraGroups or [ ]));
        message = "the gameplay user must not hold raw input, uinput, local-control, or korrid service groups.";
      }
    ];

    users.groups.${serviceGroup}.gid = cfg.gid;
    users.groups.${controlGroup}.gid = cfg.controlGid;
    users.users.${serviceUser} = {
      uid = cfg.uid;
      group = serviceGroup;
      isSystemUser = true;
    };

    environment.systemPackages = [ cfg.package ];

    systemd.tmpfiles.rules = [
      "d ${controlDirectory} 0750 root ${controlGroup} -"
      "d ${cfg.privateStateRoot} 0700 ${serviceUser} ${serviceGroup} -"
      "d ${cfg.privateStateRoot}/identity 0700 ${serviceUser} ${serviceGroup} -"
      "d /dev/inputplumber 0700 root root -"
      "d /dev/inputplumber/sources 0700 root root -"
    ];

    systemd.sockets.korrid-control = {
      description = "Private korrid exact-session control socket";
      wantedBy = [ "sockets.target" ];
      before = [ "korrid.service" ];
      requires = [ "systemd-tmpfiles-setup.service" ];
      after = [
        "systemd-tmpfiles-setup.service"
        "systemd-tmpfiles-resetup.service"
      ];
      socketConfig = {
        ListenStream = cfg.controlSocket;
        SocketUser = "root";
        SocketGroup = controlGroup;
        SocketMode = "0660";
        DirectoryMode = "0750";
        RemoveOnStop = true;
        Service = "korrid.service";
      };
    };

    systemd.services.korrid-identity = {
      description = "Prepare the private korrid device identity";
      before = [ "korrid.service" ];
      requiredBy = [ "korrid.service" ];
      environment = {
        KORRID_PRIVATE_STATE_ROOT = cfg.privateStateRoot;
      }
      // lib.optionalAttrs bundleCfg.enable {
        KORRI_BUNDLE_ACTIVE = bundleCfg.activePath;
      };
      serviceConfig = {
        Type = "oneshot";
        User = serviceUser;
        Group = serviceGroup;
        StateDirectory = "korrid";
        StateDirectoryMode = "0700";
        UMask = "0077";
        ExecStartPre = lib.optional (cfg.ownerBindingFile != null) ownerBindingValidator;
        ExecStart =
          if cfg.ownerBindingFile == null then
            "${identityExecutable} identity status"
          else
            "${identityExecutable} identity import --file ${ownerBindingStoreFile}";
        NoNewPrivileges = true;
        CapabilityBoundingSet = [ ];
        AmbientCapabilities = [ ];
        RestrictAddressFamilies = [ "AF_UNIX" ];
        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ProtectProc = "invisible";
        ProcSubset = "pid";
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = false;
        SystemCallArchitectures = "native";
        ReadWritePaths = [ cfg.privateStateRoot ];
        # Runtime directories owned by later units may not exist yet when
        # identity runs at boot. systemd fails namespace setup for a missing
        # path unless it is marked optional, so hide these only when present.
        InaccessiblePaths = [
          cfg.storageRoot
          "-${cfg.sunshinePrivateStateRoot}"
          "-${cfg.compositorControlDirectory}"
          "-${cfg.certificateControlDirectory}"
          "-/dev/inputplumber/sources"
          "/dev/uinput"
        ];
      };
    };

    systemd.services.korrid = {
      description = "Korri Linux device daemon";
      wantedBy = [ "multi-user.target" ];
      requires = [
        "korrid-control.socket"
        "korrid-identity.service"
      ]
      ++ lib.optional bundleCfg.enable "korri-bundle-selector.service";
      after = [
        "network.target"
        "korrid-control.socket"
        "korrid-identity.service"
        "korri-input-source-guard.service"
        "systemd-tmpfiles-setup-dev.service"
        "systemd-tmpfiles-resetup.service"
      ]
      ++ lib.optional bundleCfg.enable "korri-bundle-selector.service";
      environment = {
        KORRID_MODE = "host";
        KORRID_ADDRESS = cfg.address;
        KORRID_HOST_CONFIG = toString cfg.deviceConfig;
        KORRID_STORAGE_ROOT = cfg.storageRoot;
        KORRID_PRIVATE_STATE_ROOT = cfg.privateStateRoot;
        KORRID_SUNSHINE_PRIVATE_STATE_ROOT = cfg.sunshinePrivateStateRoot;
        KORRID_CONTROL_SOCKET = cfg.controlSocket;
        KORRID_CONTROL_DIRECTORY = controlDirectory;
        KORRID_COMPOSITOR_CONTROL_DIRECTORY = cfg.compositorControlDirectory;
        KORRID_CERTIFICATE_CONTROL_DIRECTORY = cfg.certificateControlDirectory;
        KORRID_CONTROL_PEER_UID = toString cfg.inputdUid;
        KORRID_CONTROL_PEER_GID = toString cfg.controlGid;
        KORRID_GAMEPLAY_UID = toString cfg.gameplayUid;
        KORRID_GAMEPLAY_GID = toString cfg.gameplayGid;
        KORRID_RELAYS = relayJson;
        KORRID_UPSTREAMS = nativePeersJson;
        KORRID_SYSTEMD_RUN = "${pkgs.systemd}/bin/systemd-run";
        KORRID_SYSTEMCTL = "${pkgs.systemd}/bin/systemctl";
      }
      // lib.optionalAttrs bundleCfg.enable {
        KORRI_BUNDLE_ACTIVE = bundleCfg.activePath;
      };
      serviceConfig = {
        ExecStart =
          if bundleCfg.enable then
            "${bundleCfg.launcherPackage}/bin/korri-bundle-launch korrid"
          else
            lib.getExe cfg.package;
        User = serviceUser;
        Group = serviceGroup;
        StateDirectory = "korrid";
        StateDirectoryMode = "0700";
        RuntimeDirectory = "korrid";
        RuntimeDirectoryMode = "0700";
        Restart = "on-failure";
        RestartSec = 1;
        NoNewPrivileges = true;
        CapabilityBoundingSet = [ ];
        AmbientCapabilities = [ ];
        RestrictAddressFamilies = [
          "AF_UNIX"
          "AF_INET"
          "AF_INET6"
        ];
        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ProtectProc = "invisible";
        ProcSubset = "pid";
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = false;
        SystemCallArchitectures = "native";
        UMask = "0077";
        ReadWritePaths = [
          cfg.privateStateRoot
          cfg.storageRoot
        ];
        InaccessiblePaths = [
          "/dev/uinput"
          "-/dev/inputplumber/sources"
          cfg.sunshinePrivateStateRoot
        ];
      };
    };

    security.polkit.enable = true;
    security.polkit.extraConfig = ''
      polkit.addRule(function(action, subject) {
        var unit = action.lookup("unit");
        if (action.id == "org.freedesktop.systemd1.manage-units" &&
            subject.user == "korrid" &&
            typeof unit == "string" && /^korri-game-[0-9a-f]{32}\.service$/.test(unit)) {
          return polkit.Result.YES;
        }
      });
    '';
  };
}
