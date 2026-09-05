{
  pkgs,
  module,
  bundleModule,
  inputdPackage,
  inputplumberKorri,
}:
let
  lib = pkgs.lib;
  legacyInputModule =
    { config, lib, ... }:
    {
      options.services.korri.input.provider.enable = lib.mkEnableOption "legacy Korri input provider";
      config = lib.mkIf config.services.korri.input.provider.enable {
        services.inputplumber = {
          enable = true;
          package = pkgs.inputplumber;
        };
      };
    };
  identities = {
    users.groups.korri.gid = 1000;
    users.users.korri = {
      isNormalUser = true;
      uid = 1000;
      group = "korri";
    };
    services.korriLinuxInput.inputd = {
      uid = 977;
      controlGid = 977;
      actionUser = "korri";
      actionUid = 1000;
      actionGid = 1000;
      package = inputdPackage;
    };
  };
  evaluate =
    extra:
    import "${pkgs.path}/nixos/lib/eval-config.nix" {
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        bundleModule
        module
        {
          system.stateVersion = "26.05";
          boot.loader.grub.enable = false;
          fileSystems."/" = {
            device = "none";
            fsType = "tmpfs";
          };
          services.korriLinuxInput.provider.package = inputplumberKorri;
        }
        extra
      ];
    };
  withInputd =
    extra:
    evaluate {
      imports = [
        identities
        extra
      ];
    };
  allAssertionsPass =
    system:
    lib.all (
      entry: if entry.assertion then true else builtins.trace entry.message false
    ) system.config.assertions;
  hasFailedAssertion =
    needle: system:
    lib.any (entry: !entry.assertion && lib.hasInfix needle entry.message) system.config.assertions;
  evaluationRejected =
    system: !(builtins.tryEval system.config.system.build.toplevel.drvPath).success;
  providerOnly = evaluate {
    services.korriLinuxInput.provider.enable = true;
  };
  legacyCompatibility = import "${pkgs.path}/nixos/lib/eval-config.nix" {
    system = pkgs.stdenv.hostPlatform.system;
    modules = [
      legacyInputModule
      bundleModule
      module
      identities
      {
        system.stateVersion = "26.05";
        boot.loader.grub.enable = false;
        fileSystems."/" = {
          device = "none";
          fsType = "tmpfs";
        };
        services.korri.input.provider.enable = false;
        services.korriLinuxInput = {
          provider = {
            enable = true;
            package = inputplumberKorri;
          };
          inputd.enable = true;
        };
      }
    ];
  };
  additionalData = pkgs.runCommand "inputplumber-module-data-fixture" { } ''
    mkdir -p "$out/share/inputplumber/devices"
    touch "$out/share/inputplumber/devices/platform-check.yaml"
  '';
  providerWithData = evaluate {
    services.korriLinuxInput.provider = {
      enable = true;
      extraDataPackages = [ additionalData ];
    };
  };
  hiddenProvider = evaluate {
    services.korriLinuxInput.provider = {
      enable = true;
      sourceHiding = {
        enable = true;
        sameFilesystem = true;
        supportedLayout = true;
      };
    };
  };
  sunshineProvider = withInputd {
    services.korriLinuxInput.provider = {
      enable = true;
      sunshine.enableUinputAccess = true;
    };
    systemd.services.sunshine.serviceConfig.SupplementaryGroups = [ "render" ];
  };
  inputdOnly = withInputd {
    services.korriLinuxInput.inputd.enable = true;
  };
  combined = withInputd {
    services.korriLinuxInput.provider.enable = true;
    services.korriLinuxInput.inputd = {
      enable = true;
      requireProvider = true;
      actions.workspace-next.command = [
        "${pkgs.coreutils}/bin/true"
        "--help"
      ];
    };
  };
  bundleFixture = pkgs.runCommand "korri-module-bundle-fixture" { } ''
    mkdir -p "$out/bin" "$out/share"
    ln -s ${inputplumberKorri}/bin/inputplumber "$out/bin/inputplumber"
    ln -s ${inputdPackage}/bin/korri-inputd "$out/bin/korri-inputd"
    ln -s ${pkgs.coreutils}/bin/true "$out/bin/korrid"
    ln -s ${inputplumberKorri}/share/inputplumber "$out/share/inputplumber"
    ln -s ${inputplumberKorri}/share/inputplumber/profiles/korri-60-xbox_one_gamepad.yaml \
      "$out/share/korri-input-profile"
  '';
  bundled = withInputd {
    services.korriBundle = {
      enable = true;
      initialPackage = bundleFixture;
      launcherPackage = inputdPackage;
    };
    services.korriLinuxInput.provider.enable = true;
    services.korriLinuxInput.inputd.enable = true;
  };
  contradictory = withInputd {
    services.korriLinuxInput.inputd = {
      enable = true;
      requireProvider = true;
    };
  };
  broad = withInputd {
    services.korriLinuxInput.inputd = {
      enable = true;
      allowBroadRawInput = true;
    };
  };
  mismatchedActionIdentity = withInputd {
    services.korriLinuxInput.inputd = {
      enable = true;
      actionUid = lib.mkForce 1002;
    };
  };
  unsafeHide = evaluate {
    services.korriLinuxInput.provider = {
      enable = true;
      sourceHiding.enable = true;
    };
  };
  invalidAction = withInputd {
    services.korriLinuxInput.inputd = {
      enable = true;
      actions.not-an-action.command = [ "${pkgs.coreutils}/bin/true" ];
    };
  };
  unreachableAction = withInputd {
    services.korriLinuxInput.inputd = {
      enable = true;
      actions.toggle-steam-visibility.command = [ "${pkgs.coreutils}/bin/true" ];
    };
  };
  destructiveAction = withInputd {
    services.korriLinuxInput.inputd = {
      enable = true;
      actions.kill-current-game.command = [ "${pkgs.coreutils}/bin/true" ];
    };
  };
  mutableAction = withInputd {
    services.korriLinuxInput.inputd = {
      enable = true;
      actions.workspace-next.command = [ "/usr/bin/true" ];
    };
  };
  dotComponentAction = withInputd {
    services.korriLinuxInput.inputd = {
      enable = true;
      actions.workspace-next.command = [ "${pkgs.coreutils}/./bin/true" ];
    };
  };
  emptyAction = withInputd {
    services.korriLinuxInput.inputd = {
      enable = true;
      actions.workspace-next.command = [ ];
    };
  };
  virtualTargetAcl = import ./virtual-target-acl.nix { inherit pkgs inputdPackage; };
  providerService = providerOnly.config.systemd.services.inputplumber;
  inputdService = inputdOnly.config.systemd.services.korri-inputd;
  combinedService = combined.config.systemd.services.korri-inputd;
  combinedEnvironment = combinedService.environment;
  combinedRules = combined.config.services.udev.extraRules;
  inputdOnlyRules = inputdOnly.config.services.udev.extraRules;
  dbusPackages = map toString combined.config.services.dbus.packages;
  bundledProvider = bundled.config.systemd.services.inputplumber;
  bundledInputd = bundled.config.systemd.services.korri-inputd;
  bundleSelector = bundled.config.systemd.services.korri-bundle-selector;
in
assert allAssertionsPass providerOnly;
assert !(providerOnly.options.services ? korri);
assert allAssertionsPass legacyCompatibility;
assert !legacyCompatibility.config.services.korri.input.provider.enable;
assert legacyCompatibility.config.services.inputplumber.enable;
assert legacyCompatibility.config.services.inputplumber.package == inputplumberKorri;
assert
  legacyCompatibility.config.systemd.services.inputplumber.environment.XDG_DATA_DIRS
  == "${inputplumberKorri}/share";
assert
  legacyCompatibility.config.systemd.services.inputplumber.serviceConfig.ExecStart
  == "${lib.getExe inputplumberKorri}";
assert legacyCompatibility.config.systemd.services ? korri-inputd;
assert providerOnly.config.services.inputplumber.enable;
assert providerOnly.config.services.inputplumber.package == inputplumberKorri;
assert builtins.elem "uinput" providerOnly.config.boot.kernelModules;
assert !(providerOnly.config.systemd.services ? korri-inputd);
assert providerService.environment.XDG_DATA_DIRS == "${inputplumberKorri}/share";
assert builtins.elem "systemd-tmpfiles-setup-dev.service"
  hiddenProvider.config.systemd.services.inputplumber.after;
assert builtins.elem "systemd-tmpfiles-resetup.service"
  hiddenProvider.config.systemd.services.inputplumber.after;
assert builtins.elem "korri-input-source-guard.service"
  hiddenProvider.config.systemd.services.inputplumber.requires;
assert hiddenProvider.config.systemd.services ? korri-input-source-guard;
assert lib.hasInfix "install -d -m 0700 -o root -g root /dev/inputplumber /dev/inputplumber/sources"
  hiddenProvider.config.systemd.services.korri-input-source-guard.serviceConfig.ExecStart;
assert builtins.elem "d /dev/inputplumber 0700 root root -"
  hiddenProvider.config.systemd.tmpfiles.rules;
assert builtins.elem "d /dev/inputplumber/sources 0700 root root -"
  hiddenProvider.config.systemd.tmpfiles.rules;
assert
  providerWithData.config.services.inputplumber.package.additionalDataPackages == [ additionalData ];
assert
  hiddenProvider.config.systemd.services.inputplumber.environment.HIDE_DEVICES_FROM_ROOT == "1";
assert
  sunshineProvider.config.systemd.services.sunshine.serviceConfig.SupplementaryGroups == [
    "render"
    "korri-sunshine-uinput"
  ];
assert
  !(builtins.elem "korri-sunshine-uinput" sunshineProvider.config.users.users.korri.extraGroups);
assert allAssertionsPass inputdOnly;
assert inputdOnly.config.systemd.services ? korri-inputd;
assert !inputdOnly.config.services.inputplumber.enable;
assert inputdService.serviceConfig.RestrictAddressFamilies == [ "AF_UNIX" ];
assert inputdService.serviceConfig.IPAddressDeny == "any";
assert builtins.elem "systemd-tmpfiles-setup-dev.service" inputdService.after;
assert builtins.elem "systemd-tmpfiles-resetup.service" inputdService.after;
assert inputdService.serviceConfig.Delegate == "pids";
assert
  inputdService.serviceConfig.CapabilityBoundingSet == [
    "CAP_SETUID"
    "CAP_SETGID"
  ];
assert
  inputdService.serviceConfig.AmbientCapabilities == [
    "CAP_SETUID"
    "CAP_SETGID"
  ];
assert builtins.elem "/dev/uinput" inputdService.serviceConfig.InaccessiblePaths;
assert inputdOnly.config.systemd.services ? korri-input-source-guard;
assert builtins.elem "korri-input-source-guard.service" inputdService.requires;
assert builtins.elem "korri-input-source-guard.service" inputdService.after;
assert lib.hasInfix "Microsoft X-Box 360 pad" inputdOnlyRules;
assert lib.hasInfix "korri-virtual-target-acl" inputdOnlyRules;
assert lib.hasInfix " grant 977 1000 $env{DEVNAME}" inputdOnlyRules;
assert lib.hasInfix "korri-virtual-target-acl" inputdService.serviceConfig.ExecStartPre;
assert lib.hasSuffix " reapply 977 1000" inputdService.serviceConfig.ExecStartPre;
assert lib.hasSuffix " revoke" inputdService.serviceConfig.ExecStopPost;
assert !(inputdService.environment ? KORRI_INPUTD_KILL_CURRENT_GAME);
assert allAssertionsPass combined;
assert builtins.elem "inputplumber.service" combinedService.after;
assert builtins.elem "inputplumber.service" combinedService.wants;
assert combined.config.users.users.korri-inputd.uid == 977;
assert combined.config.users.users.korri-inputd.group == "korri-control";
assert combinedEnvironment.KORRI_INPUTD_CONTROL_GID == "977";
assert combinedEnvironment.KORRI_INPUTD_ACTION_UID == "1000";
assert combinedEnvironment.KORRI_INPUTD_ACTION_GID == "1000";
assert
  combinedEnvironment.KORRI_INPUTD_PROFILE_PATH
  == "${inputplumberKorri}/share/inputplumber/profiles/korri-60-xbox_one_gamepad.yaml";
assert lib.hasInfix "org.shadowblip.Input.CompositeDevice.LoadProfilePath"
  combined.config.security.polkit.extraConfig;
assert lib.hasInfix "org.shadowblip.Input.CompositeDevice.SourceDevicePaths"
  combined.config.security.polkit.extraConfig;
assert lib.hasInfix "org.shadowblip.Input.CompositeDevice.Stop"
  combined.config.security.polkit.extraConfig;
assert
  (builtins.fromJSON (
    builtins.unsafeDiscardStringContext combinedEnvironment.KORRI_INPUTD_WORKSPACE_NEXT
  )) == {
    executable = "${pkgs.coreutils}/bin/true";
    argv = [ "--help" ];
    environment = { };
  };
assert lib.hasInfix "Microsoft X-Box 360 pad" combinedRules;
assert lib.hasInfix "045e" combinedRules;
assert lib.hasInfix "028e" combinedRules;
assert !(lib.hasInfix "SUBSYSTEM==\"input\", KERNEL==\"event*\", MODE" combinedRules);
assert lib.any (path: lib.hasInfix "korri-inputplumber-dbus-policy" path) dbusPackages;
assert allAssertionsPass bundled;
assert bundled.config.services.korriBundle.activePath == "/nix/var/nix/gcroots/korri-bundle/active";
assert builtins.elem "korri-bundle-selector.service" bundledProvider.requires;
assert builtins.elem "korri-bundle-selector.service" bundledInputd.requires;
assert
  bundledProvider.environment.KORRI_BUNDLE_ACTIVE == "/nix/var/nix/gcroots/korri-bundle/active";
assert bundledInputd.environment.KORRI_BUNDLE_ACTIVE == "/nix/var/nix/gcroots/korri-bundle/active";
assert
  bundledProvider.serviceConfig.ExecStart == "${inputdPackage}/bin/korri-bundle-launch inputplumber";
assert bundledInputd.serviceConfig.ExecStart == "${inputdPackage}/bin/korri-bundle-launch inputd";
assert bundleSelector.serviceConfig.UMask == "0077";
assert
  bundleSelector.serviceConfig.ExecStart
  == "${inputdPackage}/bin/korri-bundle-select initialize ${bundleFixture}";
assert hasFailedAssertion "requires its configured provider" contradictory;
assert hasFailedAssertion "broad raw-input" broad;
assert hasFailedAssertion "must exactly match the action user's primary identity"
  mismatchedActionIdentity;
assert hasFailedAssertion "same-filesystem supported layout" unsafeHide;
assert hasFailedAssertion "invalid action identifier" invalidAction;
assert hasFailedAssertion "unreachable or destructive" unreachableAction;
assert hasFailedAssertion "unreachable or destructive" destructiveAction;
assert hasFailedAssertion "immutable absolute Nix-store argv" mutableAction;
assert hasFailedAssertion "immutable absolute Nix-store argv" dotComponentAction;
assert hasFailedAssertion "immutable absolute Nix-store argv" emptyAction;
assert evaluationRejected contradictory;
assert evaluationRejected broad;
assert evaluationRejected mismatchedActionIdentity;
assert evaluationRejected unsafeHide;
assert evaluationRejected invalidAction;
assert evaluationRejected unreachableAction;
assert evaluationRejected destructiveAction;
assert evaluationRejected mutableAction;
assert evaluationRejected dotComponentAction;
assert evaluationRejected emptyAction;
pkgs.runCommand "korri-input-module-check" { } ''
  set -euo pipefail
  test -f ${providerWithData.config.services.inputplumber.package}/share/inputplumber/devices/platform-check.yaml
  candidate_gate=${inputdOnly.config.system.build.toplevel}/sw/bin/korri-device-gate
  test -x "$candidate_gate"
  test "$(sha256sum "$candidate_gate" | cut -d' ' -f1)" = \
    "$(sha256sum ${../deploy/device-check.sh} | cut -d' ' -f1)"
  policy_package="${
    lib.findFirst (path: lib.hasInfix "korri-inputplumber-dbus-policy" path) "" dbusPackages
  }"
  policy="$policy_package/share/dbus-1/system.d/korri-inputplumber.conf"
  test -f "$policy"
  grep -F '<deny own="org.shadowblip.InputPlumber"/>' "$policy" >/dev/null
  grep -F '<deny send_destination="org.shadowblip.InputPlumber" send_type="method_call"/>' "$policy" >/dev/null
  grep -F 'send_interface="org.freedesktop.DBus.Introspectable" send_member="Introspect"' "$policy" >/dev/null
  grep -F 'send_interface="org.freedesktop.DBus.Properties" send_member="Get"' "$policy" >/dev/null
  ! grep -F 'send_interface="org.freedesktop.DBus.Properties" send_member="GetAll"' "$policy" >/dev/null
  grep -F 'send_interface="org.shadowblip.Input.CompositeDevice" send_member="LoadProfilePath"' "$policy" >/dev/null
  grep -F 'send_interface="org.shadowblip.Input.CompositeDevice" send_member="Stop"' "$policy" >/dev/null

  test -x ${inputdPackage}/bin/korri-virtual-target-acl
  grep -F -- '--device-root /dev/input' ${lib.getExe virtualTargetAcl} >/dev/null
  touch "$out"
''
