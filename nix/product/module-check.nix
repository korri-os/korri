{
  pkgs,
  nixpkgs,
  korri,
  productModule,
}:
let
  inherit (pkgs) lib;
  defaultSystem = pkgs.stdenv.hostPlatform.system;
  # Share the independent reference evaluations across the hostile cases.
  # These are the two platforms exercised below, not candidate package sets.
  references = lib.genAttrs (lib.unique [ defaultSystem "aarch64-linux" ]) (
    system:
    import ./reference.nix {
      inherit nixpkgs korri productModule system;
    }
  );
  referenceForSystem = system: references.${system};
  reference = referenceForSystem defaultSystem;
  check = import ./check-lib.nix {
    inherit
      lib
      korri
      referenceForSystem
      defaultSystem
      ;
  };
  inherit (check) requirements requiredUnits;
  standalone = reference.product;
  withoutProductModule = reference.bare;
  valueAt = path: lib.attrByPath path null;
  wrongValue =
    requirement:
    if requirement ? testValue then
      requirement.testValue
    else if builtins.isBool requirement.expected then
      !requirement.expected
    else if builtins.isInt requirement.expected then
      requirement.expected + 1
    else if builtins.isString requirement.expected then
      requirement.expected + "-review-test"
    else if builtins.isList requirement.expected then
      requirement.expected ++ [ "review-test" ]
    else
      throw "product requirement ${requirement.name} needs an explicit testValue";
  forceSetting =
    requirement:
    let
      forcedValue = wrongValue requirement;
      # Locked product constants already use mkForce. Priority zero is the
      # stronger hostile definition needed to prove the gate reads their final
      # value; ordinary requirements are tested with mkForce directly.
      definition =
        if requirement.locked or false then lib.mkOverride 0 forcedValue else lib.mkForce forcedValue;
    in
    {
      inherit forcedValue;
      device = standalone.extendModules {
        modules = [ (lib.setAttrByPath requirement.path definition) ];
      };
    };
  expectedFailures =
    name: device:
    let
      deviceRequirements = check.requirementsForSystem device.pkgs.stdenv.hostPlatform.system;
      settingFailures = lib.concatMap (
        requirement:
        lib.optional (!(check.settingMatches device.config requirement)) (
          check.settingFailure name requirement
        )
      ) deviceRequirements.settings;
      # The product's patched systemd is part of the contract on every device.
      nativeFailures = lib.optional (
        toString device.config.systemd.package != deviceRequirements.nativeSystemdPackage
      ) (check.nativeSystemdPackageFailure name);
    in
    settingFailures ++ nativeFailures;
  settingTestPasses =
    requirement:
    let
      forced = forceSetting requirement;
      name = "forced-setting-${requirement.name}";
      result =
        valueAt requirement.path forced.device.config == forced.forcedValue
        && check.validate name forced.device == expectedFailures name forced.device;
      attempted = builtins.tryEval (builtins.deepSeq result result);
    in
    attempted.success && attempted.value;
  failedSettingTests = map (requirement: requirement.name) (
    builtins.filter (requirement: !(settingTestPasses requirement)) requirements.settings
  );

  forceAt =
    path: forcedValue:
    standalone.extendModules {
      modules = [ (lib.setAttrByPath path (lib.mkOverride 0 forcedValue)) ];
    };
  unitCases =
    mutationName: classPredicate: mutationFor:
    lib.concatMap (
      className:
      let
        class = check.unitClasses.${className};
      in
      lib.optionals (classPredicate class) (
        lib.concatMap (
          unitName:
          let
            mutation = mutationFor className class unitName;
          in
          lib.optional (mutation != null) (
            mutation
            // {
              inherit className unitName;
              name = mutation.name or "forced-${mutationName}-${className}-${unitName}";
              expectedFailure =
                mutation.expectedFailure
                  or (class.livenessFailure "forced-${mutationName}-${className}-${unitName}" unitName);
            }
          )
        ) requiredUnits.${className}
      )
    ) (builtins.attrNames check.unitClasses);
  disabledUnitCases = unitCases "disabled" (_: true) (
    className: class: unitName: {
      path = class.path ++ [
        unitName
        "enable"
      ];
      forcedValue = false;
      expectedFailure = class.disabledFailure "forced-disabled-${className}-${unitName}" unitName;
      name = "forced-disabled-${className}-${unitName}";
    }
  );
  conditionUnitCases = unitCases "condition" (_: true) (
    _: class: unitName: {
      path = class.path ++ [
        unitName
        "unitConfig"
        "ConditionPathExists"
      ];
      forcedValue = "/definitely-not-a-korri-product-path";
    }
  );
  assertUnitCases = unitCases "assert" (_: true) (
    _: class: unitName: {
      path = class.path ++ [
        unitName
        "unitConfig"
        "AssertPathExists"
      ];
      forcedValue = "/definitely-not-a-korri-product-path";
    }
  );
  implementationUnitCases = unitCases "implementation" (class: class.kind == "service") (
    _: class: unitName: {
      path = class.path ++ [
        unitName
        "serviceConfig"
        "ExecStart"
      ];
      forcedValue = "/bin/false";
    }
  );
  generatedHookUnitCases = unitCases "generated-pre-start" (class: class.kind == "service") (
    _: class: unitName: {
      path = class.path ++ [
        unitName
        "preStart"
      ];
      forcedValue = "exit 1";
    }
  );
  requiredServiceConfig =
    className: unitName:
    requirements.unitSignatures.${className}.${unitName}.implementation.serviceConfig;
  userAuthorityUnitCases = unitCases "authority-user" (class: class.kind == "service") (
    className: class: unitName:
    let
      expected = (requiredServiceConfig className unitName).User or null;
    in
    if expected == null then
      null
    else
      {
        path = class.path ++ [
          unitName
          "serviceConfig"
          "User"
        ];
        forcedValue = if expected == "root" then "nobody" else "root";
      }
  );
  writePathUnitCases = unitCases "write-path-removal" (class: class.kind == "service") (
    className: class: unitName:
    let
      expected = lib.toList ((requiredServiceConfig className unitName).ReadWritePaths or [ ]);
    in
    if expected == [ ] then
      null
    else
      {
        path = class.path ++ [
          unitName
          "serviceConfig"
          "ReadWritePaths"
        ];
        forcedValue = builtins.tail expected;
      }
  );
  pluginHostUserCases = builtins.filter (
    case: case.unitName == "korri-plugin-host"
  ) userAuthorityUnitCases;
  renderDeviceHookCases = builtins.filter (
    case:
    valueAt (
      check.unitClasses.${case.className}.path
      ++ [
        case.unitName
        "environment"
        "KORRI_RENDER_DEVICE"
      ]
    ) standalone.config != null
  ) generatedHookUnitCases;
  typeUnitCases = unitCases "type" (class: class.kind == "service") (
    className: class: unitName:
    let
      expected = (requiredServiceConfig className unitName).Type or null;
    in
    {
      path = class.path ++ [
        unitName
        "serviceConfig"
        "Type"
      ];
      forcedValue = if expected == "oneshot" then "simple" else "oneshot";
    }
  );
  socketListenUnitCases = unitCases "socket-listen" (class: class.kind == "socket") (
    className: class: unitName:
    let
      implementation = requirements.unitSignatures.${className}.${unitName}.implementation;
      listenFields = builtins.filter (name: lib.hasPrefix "Listen" name) (
        builtins.attrNames implementation
      );
    in
    if listenFields == [ ] then
      null
    else
      let
        field = builtins.head listenFields;
        expected = implementation.${field};
        replacement = "/run/korri-review-invalid/${unitName}";
      in
      {
        path = class.path ++ [
          unitName
          "socketConfig"
          field
        ];
        forcedValue = if builtins.isList expected then [ replacement ] else replacement;
      }
  );
  dropinUnitCases = unitCases "dropin" (_: true) (
    className: class: unitName: {
      path = class.path ++ [
        unitName
        "overrideStrategy"
      ];
      forcedValue =
        if requirements.unitSignatures.${className}.${unitName}.emission.overrideStrategy == "asDropin" then
          "asDropinIfExists"
        else
          "asDropin";
    }
  );
  activationUnitCases = unitCases "activation" (_: true) (
    className: class: unitName:
    let
      activation = requirements.unitSignatures.${className}.${unitName}.activation;
      populatedFields = builtins.filter (
        field:
        builtins.hasAttr field activation
        && builtins.isList activation.${field}
        && activation.${field} != [ ]
      ) check.activationFields;
    in
    if populatedFields == [ ] then
      null
    else
      {
        path = class.path ++ [
          unitName
          (builtins.head populatedFields)
        ];
        forcedValue = [ ];
      }
  );
  unitCasePasses =
    case:
    let
      device = forceAt case.path case.forcedValue;
      failures = check.validate case.name device;
    in
    valueAt case.path device.config == case.forcedValue
    # A hostile unit mutation can also change a dependent unit's generated
    # helper derivation. The gate must report the targeted unit; coupled
    # failures are additional valid evidence rather than a stale-test failure.
    && builtins.elem case.expectedFailure failures;
  failedUnitCases = map (case: case.name) (
    builtins.filter (case: !(unitCasePasses case)) (
      disabledUnitCases
      ++ conditionUnitCases
      ++ assertUnitCases
      ++ implementationUnitCases
      ++ generatedHookUnitCases
      ++ userAuthorityUnitCases
      ++ writePathUnitCases
      ++ typeUnitCases
      ++ socketListenUnitCases
      ++ dropinUnitCases
      ++ activationUnitCases
      ++ suppressionUnitCases
    )
  );

  suppressionUnitCases = unitCases "suppressed" (class: class.suppressible) (
    _: class: unitName:
    let
      unit = "${unitName}.${class.suffix}";
      current = standalone.config.systemd.suppressedSystemUnits;
    in
    {
      path = [
        "systemd"
        "suppressedSystemUnits"
      ];
      forcedValue =
        if builtins.elem unit current then
          builtins.filter (name: name != unit) current
        else
          current ++ [ unit ];
    }
  );
  clearedSystemdPackagesName = "cleared-systemd-packages";
  clearedSystemdPackages = forceAt [
    "systemd"
    "packages"
  ] [ ];
  clearedSystemdPackageFailures = map (
    check.systemdPackageFailure clearedSystemdPackagesName
  ) requirements.requiredSystemdPackages;

  alternateRenderDeviceName = "alternate-render-device-hardware-fact";
  alternateRenderDevicePath = [
    "services"
    "korriLinuxHost"
    "compositor"
    "renderDevice"
  ];
  alternateRenderDeviceValue = "/dev/dri/renderD129";
  alternateRenderDevice = forceAt alternateRenderDevicePath alternateRenderDeviceValue;
  referenceCompositorService = standalone.config.systemd.services.korri-compositor;
  alternateCompositorService = alternateRenderDevice.config.systemd.services.korri-compositor;

  exactExecCandidates = lib.concatMap (
    className:
    let
      class = check.unitClasses.${className};
    in
    lib.optionals (class.kind == "service") (
      lib.concatMap (
        unitName:
        let
          implementation = requiredServiceConfig className unitName;
          command = implementation.ExecStart or null;
          parts =
            if builtins.isString command then
              builtins.match "^([+!:@-]*)(/nix/store/[a-z0-9]{32}-([^/ ]+)/([^ ]+))(.*)$" command
            else
              null;
        in
        lib.optional (parts != null) {
          inherit
            class
            className
            command
            parts
            unitName
            ;
        }
      ) requiredUnits.${className}
    )
  ) (builtins.attrNames check.unitClasses);
  sameNameCandidate = builtins.head exactExecCandidates;
  sameNameOutputName = builtins.elemAt sameNameCandidate.parts 2;
  sameNameRelativeExecutable = builtins.elemAt sameNameCandidate.parts 3;
  sameNameFailingPackage = pkgs.runCommand sameNameOutputName { } ''
    mkdir -p "$out/$(dirname ${lib.escapeShellArg sameNameRelativeExecutable})"
    cat >"$out/${sameNameRelativeExecutable}" <<'SCRIPT'
    #!${pkgs.runtimeShell}
    exit 1
    SCRIPT
    chmod +x "$out/${sameNameRelativeExecutable}"
  '';
  sameNameFailingCommand = "${builtins.elemAt sameNameCandidate.parts 0}${sameNameFailingPackage}/${
    sameNameRelativeExecutable
  }${builtins.elemAt sameNameCandidate.parts 4}";
  sameNameFailingCase = {
    name = "same-name-failing-implementation";
    path = sameNameCandidate.class.path ++ [
      sameNameCandidate.unitName
      "serviceConfig"
      "ExecStart"
    ];
    forcedValue = sameNameFailingCommand;
    expectedFailure = sameNameCandidate.class.livenessFailure "same-name-failing-implementation" sameNameCandidate.unitName;
  };

  lookalikeProductOption = withoutProductModule.extendModules {
    modules = [
      {
        options.services.korriProduct.installed = lib.mkOption {
          type = lib.types.bool;
          default = true;
          readOnly = true;
        };
      }
    ];
  };
  lookalikeFailures = check.validate "lookalike-marker" lookalikeProductOption;
  fleetFailures = check.validateAll {
    valid = standalone;
    invalid = withoutProductModule;
  };
  productConfig = standalone.config;
  productPluginHost = productConfig.services.korri.pluginHost;
  productPluginRestore = productConfig.systemd.services.korri-plugin-host;
  productPluginPublicKey = requirements.constants.publishers."@korri".publicKey;
  productPluginNames = names: builtins.filter (lib.hasPrefix "korri-plugin-") names;
  productPluginDirectories = builtins.filter (lib.hasInfix "korri-plugin-host") productConfig.systemd.tmpfiles.rules;
  listensOnSsh =
    address:
    builtins.elem address [
      "22"
      "2222"
    ]
    || lib.hasSuffix ":22" address
    || lib.hasSuffix ":2222" address;
  rg353m = korri.nixosConfigurations.rg353m.config;
  # Every After= path from `unit` through NixOS-defined units that reaches one
  # of `targets`. Units a package ships are opaque here, so a package unit that
  # waits for the network must itself be one of the targets.
  unitAfter =
    config: unit:
    let
      parts = builtins.match "(.+)\\.(service|target|socket)" unit;
      kind = builtins.getAttr (builtins.elemAt parts 1) {
        service = "services";
        target = "targets";
        socket = "sockets";
      };
    in
    if parts == null then
      [ ]
    else
      (config.systemd.${kind}.${builtins.elemAt parts 0} or { }).after or [ ];
  afterPaths =
    config: targets: path: unit:
    if builtins.elem unit targets then
      [ (lib.concatStringsSep " -> " (path ++ [ unit ])) ]
    else if builtins.elem unit path then
      [ ]
    else
      lib.concatMap (afterPaths config targets (path ++ [ unit ])) (unitAfter config unit);
  # The screen and the plugin host start without the network (RG353M,
  # 2026-10-01: network.target waited for the USB controller until 24 s).
  # plymouth-quit.service is a target because its package orders it after
  # systemd-user-sessions.service, which orders after network.target.
  networkWaits =
    config:
    lib.concatMap (afterPaths config [
      "network.target"
      "network-online.target"
      "systemd-user-sessions.service"
      "plymouth-quit.service"
    ] [ ]) [
      "korri-compositor.service"
      "korri-chromium-kiosk.service"
      "korri-plugin-host.service"
    ];
  rg353mFirewallTcpPorts =
    rg353m.networking.firewall.allowedTCPPorts
    ++ lib.concatMap (interface: interface.allowedTCPPorts or [ ]) (
      builtins.attrValues rg353m.networking.firewall.interfaces
    );
  streamingTcpPorts = [
    47984
    47989
    47990
    48010
  ];

  # Replay the approved producers independently of the real board, then try
  # hostile final-value edits. The validation name stays the real export name;
  # test labels must not switch a mutation back to the default reference.
  nativeName = "rpminiv2";
  armReference = referenceForSystem "aarch64-linux";
  nativeProduct = armReference.product;
  nativeRequirements = check.requirementsForDevice nativeName nativeProduct;
  mutateNative = module: nativeProduct.extendModules { modules = [ module ]; };
  nativeUnitCases = [
    {
      unit = "korri-inputd";
      field = "BindReadOnlyPaths";
      value = [ "/run/user/${toString nativeProduct.config.services.korriLinuxHost.runtimeUid}" ];
    }
    {
      unit = "korri-inputd";
      field = "BindReadOnlyPaths";
      value = nativeProduct.config.systemd.services.korri-inputd.serviceConfig.BindReadOnlyPaths ++ [
        "/run/user/${toString nativeProduct.config.services.korriLinuxHost.runtimeUid}/bus"
      ];
    }
    {
      unit = "korri-inputd";
      field = "BindPaths";
      value = [
        "/run/user/${toString nativeProduct.config.services.korriLinuxHost.runtimeUid}/pipewire-0"
      ];
    }
    {
      unit = "korrid";
      field = "ExecStart";
      value = "/bin/false";
    }
    {
      unit = "korrid";
      field = "ExecStopPost";
      value = [ ];
    }
    {
      unit = "korrid";
      field = "ExecStopPost";
      value = [ "/bin/true" ];
    }
    {
      unit = "korrid";
      field = "CapabilityBoundingSet";
      value = [ "CAP_SYS_ADMIN" ];
    }
    {
      unit = "korrid";
      field = "ReadWritePaths";
      value = [ "/" ];
    }
    {
      unit = "korri-chromium-kiosk";
      field = "ExecStart";
      value = "/bin/false";
    }
    {
      unit = "korri-chromium-kiosk";
      field = "ExecStopPost";
      value = [ "/bin/true" ];
    }
    {
      unit = "NetworkManager";
      field = "ExecStart";
      value = "/bin/false";
    }
  ];
  nativeUnitCasePasses =
    case:
    let
      device = mutateNative {
        systemd.services.${case.unit}.serviceConfig.${case.field} = lib.mkForce case.value;
      };
    in
    device.config.systemd.services.${case.unit}.serviceConfig.${case.field} == case.value
    && builtins.elem (check.systemServiceLivenessFailure nativeName case.unit) (
      check.validate nativeName device
    );
  nativeHookMutation = mutateNative {
    systemd.services.korrid.postStop = lib.mkForce "exit 1";
  };
  nativeUserMutation = mutateNative {
    systemd.user.services.pipewire.serviceConfig.ExecStart = lib.mkForce "/bin/false";
  };
  nativePackage = nativeProduct.config.systemd.package;
  swappedNativePackage = nativePackage.overrideAttrs (old: {
    # Same version, name, and approved patch; different executable authority.
    postInstall = (old.postInstall or "") + "\necho tampered > $out/review-test\n";
  });
  nativePackageSwap = mutateNative {
    systemd.package = lib.mkForce swappedNativePackage;
  };
  nativeUnpatched = mutateNative {
    systemd.package = lib.mkForce nativeProduct.pkgs.systemd;
  };
  nativeWithoutKiosk = mutateNative {
    services.korri.compositor.kiosk.enable = lib.mkForce false;
  };
  nativeSettingCasePasses =
    requirement:
    let
      value = wrongValue requirement;
      device = mutateNative (lib.setAttrByPath requirement.path (lib.mkOverride 0 value));
    in
    valueAt requirement.path device.config == value
    && builtins.elem (check.settingFailure nativeName requirement) (check.validate nativeName device);
  nativeOnlySettings = armReference.settings;
  extraNativeAuthority = mutateNative {
    security.polkit.extraConfig = lib.mkAfter ''
      polkit.addRule(function(action, subject) { return polkit.Result.YES; });
    '';
  };
  nativePolicyRequirement = builtins.head (
    builtins.filter (
      requirement:
      requirement.path == [
        "security"
        "polkit"
        "extraConfig"
      ]
    ) nativeOnlySettings
  );
  missingNativeDbusPolicy = mutateNative {
    services.dbus.packages = lib.mkForce (
      builtins.filter (
        package: !(builtins.elem (toString package) nativeRequirements.requiredDbusPackages)
      ) nativeProduct.config.services.dbus.packages
    );
  };
  tamperedNativeDbusPolicy = missingNativeDbusPolicy.extendModules {
    modules = [
      (
        { pkgs, ... }:
        {
          services.dbus.packages = lib.mkOverride 0 (
            missingNativeDbusPolicy.config.services.dbus.packages
            ++ [
              (pkgs.writeTextDir "share/dbus-1/system.d/korri-portal-freezer.conf" ''
                <busconfig><policy user="korrid"><allow send_destination="*"/></policy></busconfig>
              '')
            ]
          );
        }
      )
    ];
  };
  # The Linux 7.2 pin: another series fails unless the device states why.
  kernelPinHolds = config: [ config.services.korriProduct.kernel.pinHolds ];
  unpinnedKernel = withoutReason: standalone.extendModules {
    modules = [
      (
        { pkgs, ... }:
        {
          boot.kernelPackages = lib.mkForce pkgs.linuxPackages_6_12;
          services.korriProduct.kernel.overrideReason = lib.mkForce withoutReason;
        }
      )
    ];
  };
in
assert kernelPinHolds (unpinnedKernel null).config == [ false ];
assert kernelPinHolds (unpinnedKernel "review-test board needs 6.12").config == [ true ];
assert (unpinnedKernel null).config.services.korriProduct.kernel.series == "7.2";
assert check.validate "standalone" standalone == [ ];
assert check.validate nativeName nativeProduct == [ ];
assert check.validate nativeName korri.nixosConfigurations.rpminiv2 == [ ];
assert
  nativeRequirements.requiredUnits == (check.requirementsForSystem "aarch64-linux").requiredUnits;
assert lib.all nativeUnitCasePasses nativeUnitCases;
assert builtins.elem (check.systemServiceLivenessFailure nativeName "korrid") (
  check.validate nativeName nativeHookMutation
);
assert builtins.elem (check.userServiceLivenessFailure nativeName "pipewire") (
  check.validate nativeName nativeUserMutation
);
assert toString swappedNativePackage != toString nativePackage;
assert swappedNativePackage.patches == nativePackage.patches;
assert builtins.elem (check.nativeSystemdPackageFailure nativeName) (
  check.validate nativeName nativePackageSwap
);
assert builtins.elem (check.nativeSystemdPackageFailure nativeName) (
  check.validate nativeName nativeUnpatched
);
# The imported producer remains inert without either prerequisite.
assert lib.all (device:
  !(device.config.systemd.services.korrid.environment ? KORRID_PORTAL_UNIT)
  && (device.config.systemd.services.korrid.serviceConfig.ExecStopPost or [ ]) == [ ]
  && !(lib.hasInfix "korri-chromium-kiosk" device.config.security.polkit.extraConfig)
  && lib.intersectLists nativeRequirements.requiredDbusPackages
    (map toString device.config.services.dbus.packages) == [ ]
) [ nativeUnpatched nativeWithoutKiosk ];
assert check.validate nativeName nativeWithoutKiosk != [ ];
# The same product contract holds for every device export.
assert check.validate "rg353m" nativeProduct == [ ];
assert lib.all nativeSettingCasePasses nativeOnlySettings;
assert builtins.elem (check.settingFailure nativeName nativePolicyRequirement) (
  check.validate nativeName extraNativeAuthority
);
assert builtins.length nativeRequirements.requiredDbusPackages == 1;
assert lib.all
  (
    device:
    builtins.elem (check.dbusPackageFailure nativeName (builtins.head nativeRequirements.requiredDbusPackages)) (
      check.validate nativeName device
    )
  )
  [
    missingNativeDbusPolicy
    tamperedNativeDbusPolicy
  ];
assert check.validate "rg353m" korri.nixosConfigurations.rg353m == [ ];
assert check.validate "rg353m-rescue" korri.nixosConfigurations.rg353m-rescue == [ ];
assert
  check.validateModule "missing-module" withoutProductModule == [
    (check.missingModuleFailure "missing-module")
  ];
# A marker-only lookalike passes the diagnostic hint but fails the complete
# behavioral contract. Fully reproducing that contract is behaviorally
# equivalent and outside this non-security evaluation gate.
assert check.validateModule "lookalike-marker" lookalikeProductOption == [ ];
assert lookalikeFailures == expectedFailures "lookalike-marker" lookalikeProductOption;
assert lookalikeFailures != [ ];
assert lib.assertMsg (
  failedSettingTests == [ ]
) "product setting negative tests failed: ${builtins.toJSON failedSettingTests}";
assert lib.assertMsg (
  failedUnitCases == [ ]
) "product unit activation/implementation negative tests failed: ${builtins.toJSON failedUnitCases}";
assert
  builtins.length disabledUnitCases
  == lib.foldl' builtins.add 0 (map builtins.length (builtins.attrValues requiredUnits));
assert builtins.length conditionUnitCases == builtins.length disabledUnitCases;
assert builtins.length assertUnitCases == builtins.length disabledUnitCases;
assert builtins.length dropinUnitCases == builtins.length disabledUnitCases;
assert builtins.any (case: case.className == "systemServices") dropinUnitCases;
assert builtins.any (case: case.className == "systemSockets") dropinUnitCases;
assert
  builtins.length implementationUnitCases
  == builtins.length requiredUnits.systemServices + builtins.length requiredUnits.userServices;
assert builtins.length generatedHookUnitCases == builtins.length implementationUnitCases;
assert builtins.length pluginHostUserCases == 1;
assert (builtins.head pluginHostUserCases).forcedValue == "nobody";
assert writePathUnitCases != [ ];
assert renderDeviceHookCases != [ ];
assert lib.all unitCasePasses renderDeviceHookCases;
assert builtins.length typeUnitCases == builtins.length implementationUnitCases;
assert socketListenUnitCases != [ ];
assert lib.all (case: check.unitClasses.${case.className}.kind == "socket") socketListenUnitCases;
assert
  lib.all (
    className: builtins.any (case: case.className == className) activationUnitCases
  ) (builtins.attrNames (lib.filterAttrs (_: units: units != [ ]) requiredUnits));
assert
  builtins.length suppressionUnitCases
  == lib.foldl' builtins.add 0 (
    map builtins.length (
      builtins.attrValues (lib.filterAttrs (name: _: check.unitClasses.${name}.suppressible) requiredUnits)
    )
  );
assert builtins.any (case: case.className == "systemServices") suppressionUnitCases;
assert builtins.any (case: case.className == "systemSockets") suppressionUnitCases;
assert check.validate clearedSystemdPackagesName clearedSystemdPackages == clearedSystemdPackageFailures;
assert clearedSystemdPackages.config.systemd.packages == [ ];
assert requirements.requiredSystemdPackages != [ ];
assert valueAt alternateRenderDevicePath alternateRenderDevice.config == alternateRenderDeviceValue;
assert
  alternateCompositorService.environment.KORRI_RENDER_DEVICE == alternateRenderDeviceValue;
assert
  alternateCompositorService.environment.KORRI_RENDER_DEVICE
  != referenceCompositorService.environment.KORRI_RENDER_DEVICE;
assert alternateCompositorService.serviceConfig == referenceCompositorService.serviceConfig;
assert check.validate alternateRenderDeviceName alternateRenderDevice == [ ];
assert lib.hasSuffix "-${sameNameOutputName}" (toString sameNameFailingPackage);
assert sameNameFailingCommand != sameNameCandidate.command;
assert unitCasePasses sameNameFailingCase;
assert builtins.elem "host validation enabled" (
  map (requirement: requirement.name) requirements.settings
);
assert builtins.elem "product audio enabled" (
  map (requirement: requirement.name) requirements.settings
);
assert builtins.elem "korrid-identity" requiredUnits.systemServices;
assert builtins.elem "korri-input-source-guard" requiredUnits.systemServices;
assert builtins.elem "NetworkManager" requiredUnits.systemServices;
assert builtins.elem "inputplumber" requiredUnits.systemServices;
assert builtins.elem "nginx" requiredUnits.systemServices;
assert builtins.elem "seatd" requiredUnits.systemServices;
assert builtins.elem "pipewire" requiredUnits.userServices;
assert builtins.elem "pipewire-pulse" requiredUnits.userServices;
assert !(builtins.elem "sunshine" requiredUnits.systemServices);
assert !(builtins.elem "sunshine" requiredUnits.userServices);
assert !(builtins.elem "korri-certificate-control" requiredUnits.systemSockets);
assert !(requirements ? devices);
assert builtins.attrNames fleetFailures == [ "invalid" ];
assert productPluginHost.enable;
assert productPluginHost.publishers == requirements.constants.publishers;
assert productConfig.services.korriProduct.sleep.states == [ ];
assert productConfig.services.logind.settings.Login.HandlePowerKey == "poweroff";
assert productConfig.services.logind.settings.Login.HandleLidSwitch == "poweroff";
assert rg353m.services.korriProduct.sleep.states == [ ];
assert
  builtins.fromJSON productConfig.environment.etc."korri-plugin-host/publishers.json".text
  == requirements.constants.publishers;
assert builtins.elem productPluginPublicKey productConfig.nix.settings.trusted-public-keys;
assert productPluginHost.officialCatalogUrl == null;
assert productConfig.nix.buildMachines == [ ];
assert builtins.elem "nix-command" productConfig.nix.settings.experimental-features;
assert !(productConfig.systemd.services ? sshd);
assert !(productConfig.systemd.services ? sshd-keygen);
assert !(productConfig.systemd.sockets ? sshd);
assert lib.all (
  socket:
  !(lib.any listensOnSsh (
    socket.listenStreams ++ lib.toList (socket.socketConfig.ListenStream or [ ])
  ))
) (lib.attrValues productConfig.systemd.sockets);
assert productConfig.users.users.root.openssh.authorizedKeys.keys == [ ];
assert productConfig.users.users.root.openssh.authorizedKeys.keyFiles == [ ];
assert productConfig.users.users.root.hashedPassword == null;
assert productConfig.users.users.root.password == null;
assert productConfig.users.users.root.initialPassword == null;
assert productConfig.users.users.root.hashedPasswordFile == null;
assert productConfig.users.users.sshd.isSystemUser;
assert productConfig.users.users.sshd.group == "sshd";
assert productConfig.users.groups ? sshd;
assert productConfig.security.pam.services.sshd.startSession;
assert !productConfig.security.pam.services.sshd.unixAuth;
assert productConfig.security.pam.services.sshd.rules.session.systemd.enable;
assert builtins.elem "tun" productConfig.boot.kernelModules;
assert
  productPluginNames (map lib.getName productConfig.environment.systemPackages) == [
    "korri-plugin-host"
  ];
assert builtins.elem productPluginHost.package productConfig.environment.systemPackages;
assert
  productPluginNames (builtins.attrNames productConfig.environment.etc) == [
    "korri-plugin-host/publishers.json"
  ];
assert
  productPluginNames (builtins.attrNames productConfig.systemd.services) == [
    "korri-plugin-host"
  ];
assert
  productPluginDirectories == [
    "d /var/lib/korri-plugin-host 0700 root root -"
    "Z /var/lib/korri-plugin-host - root root -"
    "d /run/korri-plugin-host 0755 root root -"
    "d /nix/var/nix/gcroots/korri-plugin-host 0700 root root -"
    "Z /nix/var/nix/gcroots/korri-plugin-host - root root -"
  ];
assert productPluginRestore.enable;
assert productPluginRestore.wantedBy == [ "multi-user.target" ];
assert builtins.elem "korrid.service" productPluginRestore.before;
assert lib.all (unit: builtins.elem unit productPluginRestore.after) [
  "systemd-tmpfiles-setup.service"
  "firewall.service"
];
assert lib.assertMsg (
  networkWaits productConfig == [ ]
) "product units wait for the network: ${builtins.toJSON (networkWaits productConfig)}";
assert lib.assertMsg (
  networkWaits rg353m == [ ]
) "rg353m units wait for the network: ${builtins.toJSON (networkWaits rg353m)}";
# Coldplug replays devices and named module events only (udev-coldplug-package.nix).
assert lib.all (
  config:
  let
    exec = config.systemd.services.systemd-udev-trigger.serviceConfig.ExecStart or [ ];
  in
  lib.assertMsg (
    builtins.isList exec
    && builtins.length exec == 2
    && builtins.elemAt exec 0 == ""
    && lib.hasSuffix "/bin/korri-udev-coldplug" (builtins.elemAt exec 1)
  ) "systemd-udev-trigger must run only korri-udev-coldplug: ${builtins.toJSON exec}"
) [
  productConfig
  rg353m
];
assert
  productPluginRestore.serviceConfig.ExecStart
  == "${productPluginHost.package}/bin/korri-plugin restore-all";
assert productPluginRestore.serviceConfig.Type == "oneshot";
assert productPluginRestore.serviceConfig.User == "root";
assert productPluginRestore.environment.XDG_CACHE_HOME == "/run/korri-plugin-host";
assert
  productPluginRestore.restartTriggers == [
    productConfig.environment.etc."korri-plugin-host/publishers.json".source
  ];
assert !rg353m.services.sunshine.enable;
assert !rg353m.services.sunshine.openFirewall;
assert !(rg353m.systemd.services ? sunshine);
assert !(rg353m.systemd.sockets ? korri-certificate-control);
assert !rg353m.services.korriLinuxInput.provider.sunshine.enableUinputAccess;
# Korrid owns the bounded trust effect even when the removable Sunshine plugin
# is absent. No streaming unit, device grant, or firewall port follows from
# these private optional paths.
assert
  rg353m.systemd.services.korrid.environment.KORRID_STREAM_PRIVATE_STATE_ROOT
  == "/home/korri/.config/sunshine";
assert
  rg353m.systemd.services.korrid.environment.KORRID_CERTIFICATE_CONTROL_DIRECTORY
  == "/run/korri-certificate-control";
assert
  rg353m.systemd.services.korrid.environment.KORRID_STREAM_CERTIFICATE_CONTROL_SOCKET
  == "/run/korri-certificate-control/control.sock";
assert
  rg353m.systemd.services.korrid.environment.KORRID_INPUT_SEAT_CONTROL_SOCKET
  == "/run/korri-input-seat/control.sock";
assert !(builtins.elem 22 rg353mFirewallTcpPorts);
assert lib.intersectLists streamingTcpPorts rg353mFirewallTcpPorts == [ ];
assert
  rg353m.systemd.services.korri-chromium-kiosk.environment.WAYLAND_DISPLAY
  == rg353m.systemd.services.korri-compositor.environment.KORRI_WAYLAND_DISPLAY;
assert rg353m.services.korriLinuxHost.label == "haku";
assert rg353m.services.korriLinuxHost.relays == requirements.constants.relays;
# Every product device opens Pico (Phase 2 of Pico on Korri).
assert requirements.constants.surfaceId == "pico";
assert rg353m.services.korri.webSurfaceHost.surfaceId == "pico";
assert rg353m.systemd.services.korri-chromium-kiosk.environment.KORRI_WEB_SURFACE_URL == "http://127.0.0.1:8099/?surface=pico";
assert rg353m.services.korridLinuxDevice.address == requirements.constants.korridAddress;
assert rg353m.services.korri.compositor.kiosk.extraChromiumArgs == [ "--disable-gpu" ];
assert standalone.config.services.korri.compositor.kiosk.extraChromiumArgs == [ ];
assert
  rg353m.services.korriLinuxHost.compositor.drmDevice
  == "/dev/dri/by-path/platform-display-subsystem-card";
assert rg353m.services.korriLinuxHost.compositor.renderDevice == "/dev/dri/renderD128";
assert rg353m.services.korriLinuxHost.compositor.outputName == "DSI-1";
assert rg353m.services.korriLinuxHost.compositor.mode == "640x480@60Hz";
assert rg353m.services.korriLinuxHost.compositor.localInput.enable;
assert
  map lib.getName rg353m.services.korriLinuxInput.provider.extraDataPackages == [
    "rg353m-inputplumber-data"
  ];
assert !(rg353m.users.users ? gameplay);
assert !(rg353m.users.groups ? games);
pkgs.runCommand "korri-product-module-check" { } ''
  test -x ${sameNameFailingPackage}/${sameNameRelativeExecutable}
  touch "$out"
''
