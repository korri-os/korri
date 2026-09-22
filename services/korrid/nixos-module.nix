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
  signerUser = "korri-local-signer";
  signerGroup = "korri-local-signer";
  controlGroup = "korri-control";
  controlDirectory = builtins.dirOf cfg.controlSocket;
  signerSocketDirectory = builtins.dirOf cfg.localSignerSocket;
  signerPublicDirectory = "${signerSocketDirectory}/public";
  signerPublicKeyFile = "${signerPublicDirectory}/person.pub";
  signerExpectedDeviceFile = "${signerSocketDirectory}/expected-device-public-key";
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
  validIpv6 =
    address:
    let
      compressed = lib.splitString "::" address;
      groups = lib.filter (part: part != "") (lib.splitString ":" address);
      validGroup = part: builtins.match "[0-9a-fA-F]{1,4}" part != null;
    in
    lib.all validGroup groups
    && !(lib.hasInfix ":::" address)
    && (!(lib.hasPrefix ":" address) || lib.hasPrefix "::" address)
    && (!(lib.hasSuffix ":" address) || lib.hasSuffix "::" address)
    && (
      if builtins.length compressed == 2 then
        builtins.length groups < 8
      else
        builtins.length compressed == 1
        && builtins.length groups == 8
        && !(lib.hasPrefix ":" address)
        && !(lib.hasSuffix ":" address)
    );
  validPeerOrigin =
    value:
    let
      parts = builtins.match "^https?://([A-Za-z0-9._-]+|[[][0-9a-fA-F:]+[]])(:([0-9]+))?/?$" value;
      port = if parts == null then null else builtins.elemAt parts 2;
      host = if parts == null then "" else builtins.elemAt parts 0;
    in
    builtins.stringLength value <= 2048
    && parts != null
    && (!(lib.hasPrefix "[" host) || validIpv6 (lib.removeSuffix "]" (lib.removePrefix "[" host)))
    && (
      port == null || (builtins.stringLength port <= 5 && lib.toInt port >= 1 && lib.toInt port <= 65535)
    );
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
  localSignerExecutable =
    if bundleCfg.enable then
      "${bundleCfg.launcherPackage}/bin/korri-bundle-launch local-signer"
    else
      "${cfg.package}/bin/korri-local-signer";
  signerDeviceCredentialHelper = pkgs.writeShellScript "korri-local-signer-device-credential" ''
    set -eu
    umask 077
    status="$(${identityExecutable} identity status)"
    key="$(printf '%s' "$status" | ${pkgs.jq}/bin/jq -er '
      if ((._tag == "Unowned" or ._tag == "Owned" or ._tag == "Revoked")
          and (.devicePublicKey | type == "string")
          and (.devicePublicKey | test("^[0-9a-f]{64}$")))
      then .devicePublicKey
      else error("identity status has no valid device public key")
      end
    ')"
    temporary="$(${pkgs.coreutils}/bin/mktemp ${lib.escapeShellArg signerSocketDirectory}/.expected-device-public-key.XXXXXX)"
    trap '${pkgs.coreutils}/bin/rm -f "$temporary"' EXIT
    printf '%s\n' "$key" > "$temporary"
    ${pkgs.coreutils}/bin/chmod 0400 "$temporary"
    ${pkgs.coreutils}/bin/chown root:root "$temporary"
    ${pkgs.coreutils}/bin/mv -T "$temporary" ${lib.escapeShellArg signerExpectedDeviceFile}
    ${pkgs.coreutils}/bin/sync -f ${lib.escapeShellArg signerSocketDirectory}
    trap - EXIT
  '';
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
    runtimeUser = lib.mkOption { type = lib.types.str; };
    runtimeUid = lib.mkOption { type = lib.types.ints.positive; };
    runtimeGid = lib.mkOption { type = lib.types.ints.positive; };
    inputdUid = lib.mkOption { type = lib.types.ints.positive; };
    controlGid = lib.mkOption { type = lib.types.ints.positive; };
    localSignerUid = lib.mkOption { type = lib.types.ints.positive; };
    localSignerGid = lib.mkOption { type = lib.types.ints.positive; };
    address = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1:43117";
    };
    browser = {
      enable = lib.mkEnableOption "the capability-bound local browser RPC listener";
      address = lib.mkOption {
        type = lib.types.str;
        default = "127.0.0.1:0";
        description = "Loopback address for the local browser RPC listener.";
      };
      origin = lib.mkOption {
        type = lib.types.str;
        default = "http://127.0.0.1:8099";
        description = "Exact browser origin allowed to call the local RPC listener.";
      };
      readGroup = lib.mkOption {
        type = lib.types.str;
        default = "korri";
        description = "Group allowed to read the generated browser capability file.";
      };
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
    localSignerPrivateStateRoot = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/korri-local-signer";
    };
    localSignerSocket = lib.mkOption {
      type = lib.types.str;
      default = "/run/korri-local-signer/signer.sock";
    };
    sunshinePrivateStateRoot = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Optional streaming-plugin private state directory hidden from every game unit.";
    };
    compositorControlDirectory = lib.mkOption {
      type = lib.types.str;
      default = "/run/korri-compositor";
      description = "Exact compositor control directory hidden from every game unit.";
    };
    compositorControlSocket = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = ''
        Exact Sway control socket korrid uses to bring a resumed game back to
        the front. Null leaves resume without compositor focus.
      '';
    };
    neverFocusAppIds = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = ''
        Window identities korrid must never raise as a game, such as the kiosk
        hub. The hub shares the runtime user, so process ownership alone cannot
        tell it apart from a game.
      '';
    };
    certificateControlDirectory = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Optional streaming-plugin certificate-control directory hidden from every game unit.";
    };
    controlSocket = lib.mkOption {
      type = lib.types.str;
      default = "/run/korrid-control/control.sock";
    };
    relays = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      description = "Ordered relay URLs. Production relays use wss://; ws:// is limited to loopback tests.";
    };
    advertisedEndpoints = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = "Reachable HTTP(S) peer origins advertised to verified same-owner devices. Empty is query-only.";
    };
    moonlightAddress = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Optional Moonlight address in authenticated endpoint announcements.";
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
        assertion =
          cfg.uid != cfg.runtimeUid
          && cfg.localSignerUid != cfg.runtimeUid
          && cfg.localSignerUid != cfg.uid
          && cfg.localSignerUid != cfg.inputdUid;
        message = "korrid and local-signer service UIDs must be distinct from each other and the untrusted runtime UID.";
      }
      {
        assertion =
          let
            user = config.users.users.${cfg.runtimeUser} or { };
            group = config.users.groups.${user.group or ""} or { };
          in
          (user.uid or null) == cfg.runtimeUid && (group.gid or null) == cfg.runtimeGid;
        message = "the configured runtime UID and GID must match the runtime user's primary identity exactly.";
      }
      {
        assertion =
          cfg.gid != cfg.runtimeGid
          && cfg.controlGid != cfg.runtimeGid
          && cfg.localSignerGid != cfg.runtimeGid
          && cfg.localSignerGid != cfg.gid
          && cfg.localSignerGid != cfg.controlGid;
        message = "korrid, local-signer, and local-control GIDs must remain distinct from the runtime GID and each other.";
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
          builtins.length cfg.advertisedEndpoints <= 8 && lib.all validPeerOrigin cfg.advertisedEndpoints;
        message = "korrid advertisedEndpoints must contain at most eight bounded HTTP(S) origins with no credentials, query, fragment or non-root path, and ports in 1..65535.";
      }
      {
        assertion =
          cfg.advertisedEndpoints == [ ]
          || cfg.moonlightAddress == null
          || (
            builtins.stringLength cfg.moonlightAddress >= 1
            && builtins.stringLength cfg.moonlightAddress <= 256
            && builtins.match ".*[[:cntrl:]].*" cfg.moonlightAddress == null
            && builtins.match "[[:space:]]*" cfg.moonlightAddress == null
          );
        message = "korrid advertised moonlightAddress must be null or bounded nonblank text without control characters.";
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
          validAbsolutePath cfg.privateStateRoot
          && validAbsolutePath cfg.localSignerPrivateStateRoot
          && (cfg.sunshinePrivateStateRoot == null || validAbsolutePath cfg.sunshinePrivateStateRoot);
        message = "korrid and local-signer private state roots and optional sunshine private state root must be normalized absolute paths.";
      }
      {
        assertion =
          validAbsolutePath cfg.localSignerSocket && validAbsolutePath signerSocketDirectory;
        message = "local signer socket and its directory must be normalized absolute paths.";
      }
      {
        assertion =
          cfg.privateStateRoot != cfg.localSignerPrivateStateRoot
          && !lib.hasPrefix "${cfg.privateStateRoot}/" cfg.localSignerPrivateStateRoot
          && !lib.hasPrefix "${cfg.localSignerPrivateStateRoot}/" cfg.privateStateRoot;
        message = "korrid and local-signer private state roots must not overlap.";
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
        assertion =
          cfg.certificateControlDirectory == null || validAbsolutePath cfg.certificateControlDirectory;
        message = "korrid certificateControlDirectory must be null or a normalized absolute path.";
      }
      {
        assertion = cfg.compositorControlSocket == null || validAbsolutePath cfg.compositorControlSocket;
        message = "korrid compositorControlSocket must be a normalized absolute path.";
      }
      {
        assertion = cfg.compositorControlSocket != null || cfg.neverFocusAppIds == [ ];
        message = "korrid neverFocusAppIds needs compositorControlSocket to have any effect.";
      }
      {
        # Games cannot reach the compositor because that directory is hidden
        # from every game unit. A socket outside it would undo that.
        assertion =
          cfg.compositorControlSocket == null
          || lib.hasPrefix "${cfg.compositorControlDirectory}/" cfg.compositorControlSocket;
        message = "korrid compositorControlSocket must sit inside compositorControlDirectory.";
      }
      {
        assertion = !lib.any (identity: lib.hasInfix "," identity) cfg.neverFocusAppIds;
        message = "korrid neverFocusAppIds entries must not contain a comma.";
      }
      {
        assertion = !cfg.browser.enable || lib.hasPrefix "127.0.0.1:" cfg.browser.address;
        message = "korrid browser.address must use IPv4 loopback.";
      }
      {
        assertion = !cfg.browser.enable || (config.users.groups.${cfg.browser.readGroup} or { }) != { };
        message = "korrid browser.readGroup must name an existing group.";
      }
      {
        assertion =
          let
            user = config.users.users.${cfg.runtimeUser} or { };
          in
          !(builtins.elem "input" (user.extraGroups or [ ]))
          && !(builtins.elem "uinput" (user.extraGroups or [ ]))
          && !(builtins.elem controlGroup (user.extraGroups or [ ]))
          && !(builtins.elem serviceGroup (user.extraGroups or [ ]))
          && !(builtins.elem signerGroup (user.extraGroups or [ ]));
        message = "the runtime user must not hold raw input, uinput, local-control, korrid, or local-signer service groups.";
      }
    ];

    users.groups.${serviceGroup}.gid = cfg.gid;
    users.groups.${signerGroup}.gid = cfg.localSignerGid;
    users.groups.${controlGroup}.gid = cfg.controlGid;
    users.users.${serviceUser} = {
      uid = cfg.uid;
      group = serviceGroup;
      isSystemUser = true;
    };
    users.users.${signerUser} = {
      uid = cfg.localSignerUid;
      group = signerGroup;
      isSystemUser = true;
    };

    environment.systemPackages = [ cfg.package ];

    systemd.tmpfiles.rules = [
      "d ${controlDirectory} 0750 root ${controlGroup} -"
      "d ${signerSocketDirectory} 0751 root ${serviceGroup} -"
      "d ${signerPublicDirectory} 2750 ${signerUser} ${serviceGroup} -"
      "d ${cfg.privateStateRoot} 0700 ${serviceUser} ${serviceGroup} -"
      "d ${cfg.privateStateRoot}/identity 0700 ${serviceUser} ${serviceGroup} -"
      "d ${cfg.localSignerPrivateStateRoot} 0700 ${signerUser} ${signerGroup} -"
      "d ${cfg.localSignerPrivateStateRoot}/identity 0700 ${signerUser} ${signerGroup} -"
      "d /dev/inputplumber 0700 root root -"
      "d /dev/inputplumber/sources 0700 root root -"
      # These parents must exist before any game namespace is created and
      # remain across browser restarts. Optional missing-path exclusions do
      # not protect directories that appear after a game has started.
      "d /run/korrid-browser 2750 ${serviceUser} ${
        if cfg.browser.enable then cfg.browser.readGroup else serviceGroup
      } -"
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

    systemd.sockets.korri-local-signer = {
      description = "Private Korri local person signer socket";
      wantedBy = [ "sockets.target" ];
      before = [ "korri-local-signer.service" ];
      requires = [ "systemd-tmpfiles-setup.service" ];
      after = [
        "systemd-tmpfiles-setup.service"
        "systemd-tmpfiles-resetup.service"
      ];
      socketConfig = {
        ListenStream = cfg.localSignerSocket;
        SocketUser = signerUser;
        SocketGroup = serviceGroup;
        SocketMode = "0660";
        DirectoryMode = "0751";
        RemoveOnStop = true;
        Service = "korri-local-signer.service";
      };
    };

    systemd.services.korri-local-signer-device-credential = {
      description = "Bind the local signer to the existing korrid device identity";
      before = [ "korri-local-signer.service" ];
      requiredBy = [ "korri-local-signer.service" ];
      requires = [ "korrid-identity.service" ];
      after = [
        "korrid-identity.service"
        "systemd-tmpfiles-setup.service"
        "systemd-tmpfiles-resetup.service"
      ];
      environment = {
        KORRID_PRIVATE_STATE_ROOT = cfg.privateStateRoot;
      }
      // lib.optionalAttrs bundleCfg.enable {
        KORRI_BUNDLE_ACTIVE = bundleCfg.activePath;
      };
      serviceConfig = {
        Type = "oneshot";
        User = "root";
        Group = "root";
        UMask = "0077";
        ExecStart = signerDeviceCredentialHelper;
        NoNewPrivileges = true;
        CapabilityBoundingSet = [ ];
        AmbientCapabilities = [ ];
        RestrictAddressFamilies = [ "AF_UNIX" ];
        PrivateNetwork = true;
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
        ReadOnlyPaths = [ cfg.privateStateRoot ];
        ReadWritePaths = [ signerSocketDirectory ];
        InaccessiblePaths = [
          cfg.localSignerPrivateStateRoot
          cfg.storageRoot
          "-${cfg.compositorControlDirectory}"
          "-/dev/inputplumber/sources"
          "/dev/uinput"
        ]
        ++ lib.optional (cfg.sunshinePrivateStateRoot != null) "-${cfg.sunshinePrivateStateRoot}"
        ++ lib.optional (cfg.certificateControlDirectory != null) "-${cfg.certificateControlDirectory}";
      };
    };

    systemd.services.korri-local-signer = {
      description = "Korri local person signer";
      wantedBy = [ "multi-user.target" ];
      requires = [
        "korri-local-signer.socket"
        "korri-local-signer-device-credential.service"
      ]
      ++ lib.optional bundleCfg.enable "korri-bundle-selector.service";
      after = [
        "korri-local-signer.socket"
        "korri-local-signer-device-credential.service"
        "systemd-tmpfiles-setup.service"
        "systemd-tmpfiles-resetup.service"
      ]
      ++ lib.optional bundleCfg.enable "korri-bundle-selector.service";
      before = [ "korrid.service" ];
      environment = {
        KORRI_LOCAL_SIGNER_PRIVATE_STATE_ROOT = cfg.localSignerPrivateStateRoot;
        KORRI_LOCAL_SIGNER_PEER_UID = toString cfg.uid;
        KORRI_LOCAL_SIGNER_PEER_GID = toString cfg.gid;
        KORRI_LOCAL_SIGNER_PUBLIC_KEY_FILE = signerPublicKeyFile;
      }
      // lib.optionalAttrs bundleCfg.enable {
        KORRI_BUNDLE_ACTIVE = bundleCfg.activePath;
      };
      serviceConfig = {
        ExecStart = localSignerExecutable;
        User = signerUser;
        Group = signerGroup;
        StateDirectory = "korri-local-signer";
        StateDirectoryMode = "0700";
        Restart = "on-failure";
        RestartSec = 1;
        UMask = "0027";
        LoadCredential = "expected-device-public-key:${signerExpectedDeviceFile}";
        NoNewPrivileges = true;
        CapabilityBoundingSet = [ ];
        AmbientCapabilities = [ ];
        RestrictAddressFamilies = [ "AF_UNIX" ];
        PrivateNetwork = true;
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
        ReadWritePaths = [
          cfg.localSignerPrivateStateRoot
          signerPublicDirectory
        ];
        InaccessiblePaths = [
          cfg.privateStateRoot
          cfg.storageRoot
          "-${cfg.compositorControlDirectory}"
          "-/dev/inputplumber/sources"
          "/dev/uinput"
        ]
        ++ lib.optional (cfg.sunshinePrivateStateRoot != null) "-${cfg.sunshinePrivateStateRoot}"
        ++ lib.optional (cfg.certificateControlDirectory != null) "-${cfg.certificateControlDirectory}";
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
          cfg.localSignerPrivateStateRoot
          "-${cfg.compositorControlDirectory}"
          "-/dev/inputplumber/sources"
          "/dev/uinput"
        ]
        ++ lib.optional (cfg.sunshinePrivateStateRoot != null) "-${cfg.sunshinePrivateStateRoot}"
        ++ lib.optional (cfg.certificateControlDirectory != null) "-${cfg.certificateControlDirectory}";
      };
    };

    systemd.services.korrid = {
      description = "Korri Linux device daemon";
      wantedBy = [ "multi-user.target" ];
      requires = [
        "korrid-control.socket"
        "korrid-identity.service"
        "korri-local-signer.service"
      ]
      ++ lib.optional bundleCfg.enable "korri-bundle-selector.service";
      after = [
        "network.target"
        "korrid-control.socket"
        "korrid-identity.service"
        "korri-local-signer.service"
        "korri-input-source-guard.service"
        "systemd-tmpfiles-setup-dev.service"
        "systemd-tmpfiles-resetup.service"
      ]
      ++ lib.optional bundleCfg.enable "korri-bundle-selector.service";
      environment = {
        HOSTNAME = config.networking.hostName;
        KORRID_ADVERTISED_ENDPOINTS = builtins.toJSON cfg.advertisedEndpoints;
        KORRID_MODE = "host";
        KORRID_ADDRESS = cfg.address;
        KORRID_HOST_CONFIG = toString cfg.deviceConfig;
        KORRID_STORAGE_ROOT = cfg.storageRoot;
        KORRID_PRIVATE_STATE_ROOT = cfg.privateStateRoot;
        KORRID_LOCAL_SIGNER_SOCKET = cfg.localSignerSocket;
        KORRID_LOCAL_SIGNER_PUBLIC_KEY_FILE = signerPublicKeyFile;
        KORRID_CONTROL_SOCKET = cfg.controlSocket;
        KORRID_CONTROL_DIRECTORY = controlDirectory;
        KORRID_COMPOSITOR_CONTROL_DIRECTORY = cfg.compositorControlDirectory;
        KORRID_CONTROL_PEER_UID = toString cfg.inputdUid;
        KORRID_CONTROL_PEER_GID = toString cfg.controlGid;
        KORRID_RUNTIME_UID = toString cfg.runtimeUid;
        KORRID_RUNTIME_GID = toString cfg.runtimeGid;
        KORRID_RELAYS = relayJson;
        KORRID_UPSTREAMS = nativePeersJson;
        KORRID_SYSTEMD_RUN = "${pkgs.systemd}/bin/systemd-run";
        KORRID_SYSTEMCTL = "${pkgs.systemd}/bin/systemctl";
      }
      // lib.optionalAttrs (cfg.sunshinePrivateStateRoot != null) {
        KORRID_SUNSHINE_PRIVATE_STATE_ROOT = cfg.sunshinePrivateStateRoot;
      }
      // lib.optionalAttrs (cfg.certificateControlDirectory != null) {
        KORRID_CERTIFICATE_CONTROL_DIRECTORY = cfg.certificateControlDirectory;
      }
      // lib.optionalAttrs (cfg.moonlightAddress != null) {
        KORRID_MOONLIGHT_ADDRESS = cfg.moonlightAddress;
      }
      // lib.optionalAttrs (cfg.compositorControlSocket != null) {
        KORRID_SWAYMSG = "${pkgs.sway}/bin/swaymsg";
        KORRID_COMPOSITOR_CONTROL_SOCKET = cfg.compositorControlSocket;
        KORRID_NEVER_FOCUS_APP_IDS = lib.concatStringsSep "," cfg.neverFocusAppIds;
      }
      // lib.optionalAttrs cfg.browser.enable {
        KORRID_BROWSER_ADDRESS = cfg.browser.address;
        KORRID_BROWSER_ORIGIN = cfg.browser.origin;
        KORRID_BROWSER_INFO_PATH = "/run/korrid-browser/brain.json";
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
        ExecStartPre = lib.optional cfg.browser.enable (
          pkgs.writeShellScript "korrid-remove-stale-browser-runtime" ''
            rm -f /run/korrid-browser/brain.json
          ''
        );
        User = serviceUser;
        Group = serviceGroup;
        # Reaching the compositor socket is what lets korrid raise a resumed
        # game. Games stay shut out by namespace hiding, not by this group.
        SupplementaryGroups = lib.optional (
          cfg.compositorControlSocket != null
        ) config.users.users.${cfg.runtimeUser}.group;
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
        ]
        ++ lib.optional cfg.browser.enable "/run/korrid-browser";
        InaccessiblePaths = [
          "/dev/uinput"
          "-/dev/inputplumber/sources"
          cfg.localSignerPrivateStateRoot
        ]
        ++ lib.optional (cfg.sunshinePrivateStateRoot != null) cfg.sunshinePrivateStateRoot;
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
