{ korri }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.korriLinuxInput;
  bundleCfg =
    config.services.korriBundle or {
      enable = false;
      activePath = "";
      launcherPackage = null;
    };
  system = pkgs.stdenv.hostPlatform.system;
  defaultInputd = korri.packages.${system}.korri-inputd;
  defaultProvider = korri.packages.${system}.inputplumber-korri;
  data = import ./inputplumber-data.nix { inherit pkgs; };
  providerPackage =
    if cfg.provider.extraDataPackages == [ ] then
      cfg.provider.package
    else
      data.composeResolved {
        inputplumberKorri = cfg.provider.package;
        additionalDataPackages = cfg.provider.extraDataPackages;
      };
  inputdUser = "korri-inputd";
  controlGroup = "korri-control";
  sunshineGroup = "korri-sunshine-uinput";
  virtualTargetAcl = import ./virtual-target-acl.nix {
    inherit pkgs;
    inputdPackage = cfg.inputd.package;
  };
  actionNames = [
    "system-panel"
    "volume-up"
    "volume-down"
    "brightness-up"
    "brightness-down"
    "screen-switch"
    "toggle-bottom-screen"
    "toggle-top-screen"
    "workspace-prev"
    "workspace-next"
    "move-output-up"
    "move-output-down"
    "toggle-bottom-keyboard"
    "toggle-steam-visibility"
  ];
  alwaysReachable = lib.remove "toggle-steam-visibility" actionNames;
  actionEnvironmentNames = {
    system-panel = "KORRI_INPUTD_SYSTEM_PANEL";
    volume-up = "KORRI_INPUTD_VOLUME_UP";
    volume-down = "KORRI_INPUTD_VOLUME_DOWN";
    brightness-up = "KORRI_INPUTD_BRIGHTNESS_UP";
    brightness-down = "KORRI_INPUTD_BRIGHTNESS_DOWN";
    screen-switch = "KORRI_INPUTD_SCREEN_SWITCH";
    toggle-bottom-screen = "KORRI_INPUTD_TOGGLE_BOTTOM_SCREEN";
    toggle-top-screen = "KORRI_INPUTD_TOGGLE_TOP_SCREEN";
    workspace-prev = "KORRI_INPUTD_WORKSPACE_PREV";
    workspace-next = "KORRI_INPUTD_WORKSPACE_NEXT";
    move-output-up = "KORRI_INPUTD_MOVE_OUTPUT_UP";
    move-output-down = "KORRI_INPUTD_MOVE_OUTPUT_DOWN";
    toggle-bottom-keyboard = "KORRI_INPUTD_BOTTOM_KEYBOARD";
    toggle-steam-visibility = "KORRI_INPUTD_TOGGLE_STEAM_VISIBILITY";
  };
  configuredActionNames = builtins.attrNames cfg.inputd.actions;
  reachableActionNames =
    alwaysReachable
    ++ lib.optional (cfg.inputd.backTapAction == "toggle-steam-visibility") "toggle-steam-visibility";
  validCommand =
    action:
    action.command != [ ]
    && lib.hasPrefix "/nix/store/" (builtins.head action.command)
    && lib.all (component: component != "." && component != "..") (
      lib.splitString "/" (builtins.head action.command)
    );
  renderableActions = lib.filterAttrs (
    name: action: builtins.hasAttr name actionEnvironmentNames && action.command != [ ]
  ) cfg.inputd.actions;
  actionEnvironment = lib.mapAttrs' (
    name: action:
    lib.nameValuePair actionEnvironmentNames.${name} (
      builtins.toJSON {
        executable = builtins.head action.command;
        argv = builtins.tail action.command;
        environment = action.environment;
      }
    )
  ) renderableActions;
  providerDbusPolicy = pkgs.writeTextFile {
    name = "korri-inputplumber-dbus-policy";
    destination = "/share/dbus-1/system.d/korri-inputplumber.conf";
    text = ''
      <!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
       "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
      <busconfig>
        <policy user="${cfg.inputd.actionUser}">
          <deny own="org.shadowblip.InputPlumber"/>
          <deny send_destination="org.shadowblip.InputPlumber" send_type="method_call"/>
        </policy>
        <policy user="${inputdUser}">
          <deny own="org.shadowblip.InputPlumber"/>
          <deny send_destination="org.shadowblip.InputPlumber" send_type="method_call"/>
          <allow send_destination="org.shadowblip.InputPlumber" send_interface="org.freedesktop.DBus.Introspectable" send_member="Introspect"/>
          <allow send_destination="org.shadowblip.InputPlumber" send_interface="org.freedesktop.DBus.Properties" send_member="Get"/>
          <allow send_destination="org.shadowblip.InputPlumber" send_interface="org.shadowblip.Input.CompositeDevice" send_member="LoadProfilePath"/>
          <allow send_destination="org.shadowblip.InputPlumber" send_interface="org.shadowblip.Input.CompositeDevice" send_member="Stop"/>
          <allow receive_sender="org.shadowblip.InputPlumber"/>
        </policy>
      </busconfig>
    '';
  };
  actionType = lib.types.submodule {
    options = {
      command = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        description = "Immutable absolute executable followed by its argv.";
      };
      environment = lib.mkOption {
        type = lib.types.attrsOf lib.types.str;
        default = { };
        description = "Explicit allowlisted environment for this action.";
      };
    };
  };
in
{
  options.services.korriLinuxInput = {
    provider = {
      enable = lib.mkEnableOption "Korri InputPlumber normalized-input provider";
      package = lib.mkOption {
        type = lib.types.package;
        default = defaultProvider;
        defaultText = lib.literalExpression "korri.packages.${system}.inputplumber-korri";
      };
      extraDataPackages = lib.mkOption {
        type = lib.types.listOf lib.types.package;
        default = [ ];
      };
      sourceHiding = {
        enable = lib.mkEnableOption "InputPlumber moved-source hiding";
        sameFilesystem = lib.mkOption {
          type = lib.types.bool;
          default = false;
        };
        supportedLayout = lib.mkOption {
          type = lib.types.bool;
          default = false;
        };
      };
      sunshine = {
        enableUinputAccess = lib.mkEnableOption "service-specific Sunshine uinput access";
        serviceName = lib.mkOption {
          type = lib.types.str;
          default = "sunshine";
        };
        gid = lib.mkOption {
          type = lib.types.ints.positive;
          default = 979;
        };
      };
    };
    inputd = {
      enable = lib.mkEnableOption "Korri Linux input policy daemon";
      package = lib.mkOption {
        type = lib.types.package;
        default = defaultInputd;
        defaultText = lib.literalExpression "korri.packages.${system}.korri-inputd";
      };
      requireProvider = lib.mkOption {
        type = lib.types.bool;
        default = false;
      };
      uid = lib.mkOption { type = lib.types.ints.positive; };
      controlGid = lib.mkOption { type = lib.types.ints.positive; };
      actionUser = lib.mkOption { type = lib.types.str; };
      actionUid = lib.mkOption { type = lib.types.ints.positive; };
      actionGid = lib.mkOption { type = lib.types.ints.positive; };
      controlSocket = lib.mkOption {
        type = lib.types.str;
        default = "/run/korrid-control/control.sock";
      };
      allowBroadRawInput = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Unsupported broad raw-input access request; true is rejected.";
      };
      backTapAction = lib.mkOption {
        type = lib.types.nullOr (
          lib.types.enum [
            "toggle-bottom-keyboard"
            "toggle-steam-visibility"
          ]
        );
        default = null;
      };
      actions = lib.mkOption {
        type = lib.types.attrsOf actionType;
        default = { };
      };
    };
  };

  config = lib.mkMerge [
    (lib.mkIf cfg.provider.enable {
      assertions = [
        {
          assertion =
            !cfg.provider.sourceHiding.enable
            || (cfg.provider.sourceHiding.sameFilesystem && cfg.provider.sourceHiding.supportedLayout);
          message = "InputPlumber moved-source hiding requires an asserted same-filesystem supported layout.";
        }
      ];
      boot.kernelModules = [ "uinput" ];
      services.inputplumber = {
        enable = true;
        package = providerPackage;
      };
      systemd.services.inputplumber = {
        requires =
          lib.optional cfg.provider.sourceHiding.enable "korri-input-source-guard.service"
          ++ lib.optional bundleCfg.enable "korri-bundle-selector.service";
        after =
          lib.optionals cfg.provider.sourceHiding.enable [
            "korri-input-source-guard.service"
            "systemd-tmpfiles-setup-dev.service"
            "systemd-tmpfiles-resetup.service"
          ]
          ++ lib.optional bundleCfg.enable "korri-bundle-selector.service";
        environment = {
          XDG_DATA_DIRS = lib.mkForce "${providerPackage}/share";
          HIDE_DEVICES_FROM_ROOT = lib.mkIf cfg.provider.sourceHiding.enable "1";
          KORRI_BUNDLE_ACTIVE = lib.mkIf bundleCfg.enable bundleCfg.activePath;
        };
        serviceConfig.ExecStart = lib.mkIf bundleCfg.enable (
          lib.mkForce "${bundleCfg.launcherPackage}/bin/korri-bundle-launch inputplumber"
        );
      };
      systemd.tmpfiles.rules = lib.optionals cfg.provider.sourceHiding.enable [
        "d /dev/inputplumber 0700 root root -"
        "d /dev/inputplumber/sources 0700 root root -"
      ];
      services.udev.extraRules = ''
        # InputPlumber is root. Sunshine receives uinput only through its own service group.
        KERNEL=="uinput", SUBSYSTEM=="misc", OWNER="root", GROUP="${
          if cfg.provider.sunshine.enableUinputAccess then sunshineGroup else "root"
        }", MODE="${
          if cfg.provider.sunshine.enableUinputAccess then "0660" else "0600"
        }", OPTIONS+="static_node=uinput"

      '';
      services.dbus.packages = [ providerPackage ];
      security.polkit.enable = true;
    })
    (lib.mkIf (cfg.inputd.enable || (cfg.provider.enable && cfg.provider.sourceHiding.enable)) {
      systemd.services.korri-input-source-guard = {
        description = "Korri hidden input source guard";
        wantedBy = [ "multi-user.target" ];
        before = [
          "inputplumber.service"
          "korri-inputd.service"
          "korrid.service"
        ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          ExecStart = "${pkgs.coreutils}/bin/install -d -m 0700 -o root -g root /dev/inputplumber /dev/inputplumber/sources";
          ExecStop = "-${pkgs.coreutils}/bin/rmdir /dev/inputplumber/sources /dev/inputplumber";
        };
      };
    })
    (lib.mkIf cfg.provider.sunshine.enableUinputAccess {
      users.groups.${sunshineGroup}.gid = cfg.provider.sunshine.gid;
      systemd.services.${cfg.provider.sunshine.serviceName}.serviceConfig.SupplementaryGroups =
        lib.mkAfter
          [
            sunshineGroup
          ];
    })
    (lib.mkIf cfg.inputd.enable {
      assertions = [
        {
          assertion = !cfg.inputd.requireProvider || cfg.provider.enable;
          message = "korri inputd requires its configured provider, but provider.enable is false.";
        }
        {
          assertion = !cfg.inputd.allowBroadRawInput;
          message = "korri inputd cannot request broad raw-input group access.";
        }
        {
          assertion = cfg.inputd.actionGid != cfg.inputd.controlGid;
          message = "inputd action GID must differ from the primary control GID.";
        }
        {
          assertion =
            let
              user = config.users.users.${cfg.inputd.actionUser} or { };
              group = config.users.groups.${user.group or ""} or { };
            in
            (user.uid or null) == cfg.inputd.actionUid && (group.gid or null) == cfg.inputd.actionGid;
          message = "configured action UID/GID must exactly match the action user's primary identity.";
        }
        {
          assertion = lib.all (name: builtins.elem name actionNames) configuredActionNames;
          message = "inputd action configuration contains an invalid action identifier.";
        }
        {
          assertion = lib.all (name: builtins.elem name reachableActionNames) configuredActionNames;
          message = "inputd action configuration contains an unreachable or destructive command override.";
        }
        {
          assertion = lib.all (name: validCommand cfg.inputd.actions.${name}) configuredActionNames;
          message = "inputd action commands must use immutable absolute Nix-store argv without traversal.";
        }
        {
          assertion = lib.all (name: builtins.match "[A-Za-z_][A-Za-z0-9_]*" name != null) (
            lib.concatMap (
              name: builtins.attrNames cfg.inputd.actions.${name}.environment
            ) configuredActionNames
          );
          message = "inputd action environment contains an invalid name.";
        }
        {
          assertion =
            let
              user = config.users.users.${cfg.inputd.actionUser} or { };
            in
            !(builtins.elem "input" (user.extraGroups or [ ]))
            && !(builtins.elem "uinput" (user.extraGroups or [ ]));
          message = "the action user must not belong to input or uinput groups.";
        }
      ];
      users.groups.${controlGroup}.gid = cfg.inputd.controlGid;
      users.users.${inputdUser} = {
        uid = cfg.inputd.uid;
        group = controlGroup;
        isSystemUser = true;
      };
      environment.systemPackages = [ cfg.inputd.package ];
      # The helper repeats this complete match before changing an ACL. The udev
      # match limits hotplug invocation and sets a closed base mode first.
      services.udev.extraRules = ''
        SUBSYSTEM=="input", KERNEL=="event*", ATTRS{name}=="Microsoft X-Box 360 pad", ATTRS{id/bustype}=="0003", ATTRS{id/vendor}=="045e", ATTRS{id/product}=="028e", ATTRS{id/version}=="0001", OWNER="root", GROUP="root", MODE="0600", RUN+="${lib.getExe virtualTargetAcl} grant ${toString cfg.inputd.uid} ${toString cfg.inputd.actionUid} $env{DEVNAME}"
      '';
      services.dbus.packages = [ providerDbusPolicy ];
      security.polkit.enable = true;
      security.polkit.extraConfig = ''
        polkit.addRule(function(action, subject) {
          if (subject.user == "${cfg.inputd.actionUser}" && action.id.indexOf("org.shadowblip.") == 0) {
            return polkit.Result.NO;
          }
          if (subject.user == "${inputdUser}" && [
            "org.shadowblip.Input.CompositeDevice.DbusDevices",
            "org.shadowblip.Input.CompositeDevice.ProfilePath",
            "org.shadowblip.Input.CompositeDevice.SourceDevicePaths",
            "org.shadowblip.Input.CompositeDevice.LoadProfilePath",
            "org.shadowblip.Input.CompositeDevice.Stop"
          ].indexOf(action.id) >= 0) {
            return polkit.Result.YES;
          }
        });
      '';
      systemd.services.korri-inputd = {
        description = "Korri input policy daemon";
        wantedBy = [ "multi-user.target" ];
        wants = lib.optional cfg.provider.enable "inputplumber.service";
        requires = [
          "korri-input-source-guard.service"
        ]
        ++ lib.optional bundleCfg.enable "korri-bundle-selector.service";
        after = [
          "dbus.service"
          "korri-input-source-guard.service"
          "systemd-udevd.service"
          "systemd-tmpfiles-setup-dev.service"
          "systemd-tmpfiles-resetup.service"
        ]
        ++ lib.optional cfg.provider.enable "inputplumber.service"
        ++ lib.optional bundleCfg.enable "korri-bundle-selector.service";
        environment = actionEnvironment // {
          KORRI_INPUTD_ACTION_UID = toString cfg.inputd.actionUid;
          KORRI_INPUTD_ACTION_GID = toString cfg.inputd.actionGid;
          KORRI_INPUTD_CONTROL_GID = toString cfg.inputd.controlGid;
          KORRI_INPUTD_CONTROL_SOCKET = cfg.inputd.controlSocket;
          KORRI_INPUTD_PROFILE_PATH = "${providerPackage}/share/inputplumber/profiles/${data.resolvedProfile}";
          KORRI_INPUTD_BACK_TAP_ACTION = lib.mkIf (cfg.inputd.backTapAction != null) cfg.inputd.backTapAction;
          KORRI_BUNDLE_ACTIVE = lib.mkIf bundleCfg.enable bundleCfg.activePath;
        };
        serviceConfig = {
          Type = "notify";
          NotifyAccess = "main";
          ExecStartPre = "+${lib.getExe virtualTargetAcl} reapply ${toString cfg.inputd.uid} ${toString cfg.inputd.actionUid}";
          ExecStart =
            if bundleCfg.enable then
              "${bundleCfg.launcherPackage}/bin/korri-bundle-launch inputd"
            else
              lib.getExe cfg.inputd.package;
          ExecStopPost = "+${lib.getExe virtualTargetAcl} revoke";
          User = inputdUser;
          Group = controlGroup;
          Restart = "on-failure";
          RestartSec = 1;
          Delegate = "pids";
          CapabilityBoundingSet = [
            "CAP_SETUID"
            "CAP_SETGID"
          ];
          AmbientCapabilities = [
            "CAP_SETUID"
            "CAP_SETGID"
          ];
          NoNewPrivileges = false;
          RestrictAddressFamilies = [ "AF_UNIX" ];
          IPAddressDeny = "any";
          PrivateTmp = true;
          PrivateDevices = false;
          ProtectSystem = "strict";
          ProtectHome = true;
          ProtectKernelTunables = true;
          ProtectKernelModules = true;
          ProtectKernelLogs = true;
          ProtectControlGroups = false;
          RestrictSUIDSGID = true;
          LockPersonality = true;
          MemoryDenyWriteExecute = true;
          SystemCallArchitectures = "native";
          UMask = "0077";
          RuntimeDirectory = "korri-inputd";
          RuntimeDirectoryMode = "0700";
          ReadWritePaths = [ "/run/korri-inputd" ];
          InaccessiblePaths = [
            "/dev/uinput"
            "-/run/korri-input-seat"
            "/dev/inputplumber/sources"
            "/var/lib/korrid"
          ];
        };
      };
    })
  ];
}
