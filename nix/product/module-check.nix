{
  pkgs,
  nixpkgs,
  korri,
  productModule,
}:
let
  inherit (pkgs) lib;
  defaultSystem = pkgs.stdenv.hostPlatform.system;
  referenceForSystem =
    system:
    import ./reference.nix {
      inherit nixpkgs korri productModule system;
    };
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
    in
    settingFailures;
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
in
assert check.validate "standalone" standalone == [ ];
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
  "network.target"
  "firewall.service"
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
assert rg353m.services.korri.webSurfaceHost.surfaceId == requirements.constants.surfaceId;
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
