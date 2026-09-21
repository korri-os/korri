{
  lib,
  korri,
  referenceForSystem,
  defaultSystem,
}:
let
  declaredRequirements = import ./requirements.nix { inherit korri; };
  valueAt = path: default: attrs: lib.attrByPath path default attrs;
  pathName = path: lib.concatStringsSep "." path;

  settingFailure =
    name: requirement:
    "${name} violates product requirement ${requirement.name} (${pathName requirement.path})";
  systemServiceFailure =
    name: service: "${name} disables or omits required product service ${service}";
  systemSocketFailure = name: socket: "${name} disables or omits required product socket ${socket}";
  userServiceFailure =
    name: service: "${name} disables or omits required product user service ${service}";
  userSocketFailure =
    name: socket: "${name} disables or omits required product user socket ${socket}";
  systemServiceLivenessFailure =
    name: service: "${name} changes activation or implementation of required product service ${service}";
  systemSocketLivenessFailure =
    name: socket: "${name} changes activation or implementation of required product socket ${socket}";
  userServiceLivenessFailure =
    name: service:
    "${name} changes activation or implementation of required product user service ${service}";
  userSocketLivenessFailure =
    name: socket: "${name} changes activation or implementation of required product user socket ${socket}";
  missingModuleFailure = name: "${name} does not expose the internal Korri product marker";
  systemdPackageFailure =
    name: package: "${name} omits required product systemd package contribution ${package}";

  settingMatches = config: requirement: valueAt requirement.path null config == requirement.expected;
  unitEnabled = unit: unit != null && (unit.enable or true);
  activeUnitNames =
    path: config:
    builtins.filter (name: unitEnabled (valueAt (path ++ [ name ]) null config)) (
      builtins.attrNames (valueAt path { } config)
    );
  addedActiveUnits =
    reference: path:
    lib.subtractLists (activeUnitNames path reference.bare.config) (
      activeUnitNames path reference.product.config
    );

  # This is an evaluation-time product-liveness and service-authority
  # signature, not a promise of runtime or hardware correctness. Preserve exact
  # service configuration and executable derivation paths within each target
  # system so a same-name replacement cannot impersonate the product.
  normalizeValue =
    value:
    if lib.isDerivation value || builtins.isPath value then
      toString value
    else if builtins.isString value then
      value
    else if builtins.isList value then
      map normalizeValue value
    else if builtins.isAttrs value then
      lib.mapAttrs (_: normalizeValue) value
    else
      value;
  definedAttrsMatching =
    predicate: attrs:
    lib.mapAttrs (_: normalizeValue) (lib.filterAttrs (name: _: predicate name) attrs);
  activationFields = [
    "wantedBy"
    "requiredBy"
    "upheldBy"
    "aliases"
    "upholds"
    "requires"
    "wants"
    "requisite"
    "bindsTo"
    "partOf"
    "startAt"
    "notSocketActivated"
  ];
  activationUnitConfigFields = [
    "Wants"
    "Requires"
    "Requisite"
    "BindsTo"
    "PartOf"
    "Upholds"
    "OnSuccess"
    "OnFailure"
  ];
  activationSignature =
    unit:
    (definedAttrsMatching (name: builtins.elem name activationFields) unit)
    // {
      unitConfig = definedAttrsMatching (
        name: builtins.elem name activationUnitConfigFields
      ) (unit.unitConfig or { });
    };
  conditionSignature =
    unit:
    definedAttrsMatching (
      name: lib.hasPrefix "Condition" name || lib.hasPrefix "Assert" name
    ) (unit.unitConfig or { });
  generatedExecSourceFields = [
    # NixOS turns this into PATH; it is executable authority rather than
    # device environment data and must stay exact for required services.
    "path"
    "script"
    "scriptArgs"
    "preStart"
    "postStart"
    "reload"
    "preStop"
    "postStop"
    "enableStrictShellChecks"
    "jobScripts"
  ];
  serviceImplementationSignature =
    unit: {
      # Compare the complete systemd service authority, sandbox, namespace,
      # path-access, identity, directory, capability, lifecycle, and executable
      # contract. No serviceConfig field is excluded. Grounded device facts are
      # carried separately in unit environment/data so they remain variable
      # without weakening product-owned service authority.
      serviceConfig = normalizeValue (unit.serviceConfig or { });
      # Preserve both the raw NixOS hook sources and their exact generated
      # derivations so generated commands cannot hide executable replacement.
      generatedExecSources = definedAttrsMatching (
        name: builtins.elem name generatedExecSourceFields
      ) unit;
    };
  socketImplementationSignature =
    unit:
    definedAttrsMatching (
      name:
      lib.hasPrefix "Listen" name
      || builtins.elem name [
        "Accept"
        "Service"
      ]
    ) (unit.socketConfig or { });
  unitSignature =
    kind: unit: emission: {
      activation = activationSignature unit;
      conditions = conditionSignature unit;
      inherit emission;
      implementation =
        if kind == "service" then
          serviceImplementationSignature unit
        else
          socketImplementationSignature unit;
    };
  unitClasses = {
    systemServices = {
      path = [
        "systemd"
        "services"
      ];
      kind = "service";
      suffix = "service";
      suppressible = true;
      disabledFailure = systemServiceFailure;
      livenessFailure = systemServiceLivenessFailure;
    };
    systemSockets = {
      path = [
        "systemd"
        "sockets"
      ];
      kind = "socket";
      suffix = "socket";
      suppressible = true;
      disabledFailure = systemSocketFailure;
      livenessFailure = systemSocketLivenessFailure;
    };
    userServices = {
      path = [
        "systemd"
        "user"
        "services"
      ];
      kind = "service";
      suffix = "service";
      suppressible = false;
      disabledFailure = userServiceFailure;
      livenessFailure = userServiceLivenessFailure;
    };
    userSockets = {
      path = [
        "systemd"
        "user"
        "sockets"
      ];
      kind = "socket";
      suffix = "socket";
      suppressible = false;
      disabledFailure = userSocketFailure;
      livenessFailure = userSocketLivenessFailure;
    };
  };
  requirementsForReference =
    reference:
    let
      requiredUnits = lib.mapAttrs (_: class: addedActiveUnits reference class.path) unitClasses;
      unitSignatures = lib.mapAttrs (
        className: class:
        lib.genAttrs requiredUnits.${className} (
          unitName:
          let
            unit = valueAt (class.path ++ [ unitName ]) null reference.product.config;
            suppressed =
              class.suppressible
              && builtins.elem "${unitName}.${class.suffix}" reference.product.config.systemd.suppressedSystemUnits;
          in
          unitSignature class.kind unit {
            inherit suppressed;
            overrideStrategy = unit.overrideStrategy or null;
          }
        )
      ) unitClasses;
      # NixOS uses systemd.packages for both system and user unit generation.
      # Keep only the product-vs-bare contribution; device additions are free.
      requiredSystemdPackages = lib.subtractLists (
        map toString reference.bare.config.systemd.packages
      ) (map toString reference.product.config.systemd.packages);
    in
    declaredRequirements
    // {
      inherit requiredSystemdPackages requiredUnits unitSignatures;
    };
  requirementsForSystem = system: requirementsForReference (referenceForSystem system);
  requirements = requirementsForSystem defaultSystem;
  inherit (requirements) requiredUnits unitSignatures;

  deviceSystem = device: device.pkgs.stdenv.hostPlatform.system;
  requirementsForDevice = device: requirementsForSystem (deviceSystem device);

  validateSetting =
    name: device: requirement:
    lib.optional (!(settingMatches (device.config or { }) requirement)) (
      settingFailure name requirement
    );
  validateUnit =
    class: requiredSignature: name: device: unitName:
    let
      config = device.config or { };
      unit = valueAt (class.path ++ [ unitName ]) null config;
      suppressed =
        class.suppressible
        && builtins.elem "${unitName}.${class.suffix}" (config.systemd.suppressedSystemUnits or [ ]);
    in
    if !(unitEnabled unit) then
      [ (class.disabledFailure name unitName) ]
    else
      lib.optional (
        unitSignature class.kind unit {
          inherit suppressed;
          overrideStrategy = unit.overrideStrategy or null;
        }
        != requiredSignature
      ) (class.livenessFailure name unitName);
  validateUnitClass =
    className: name: device: deviceRequirements:
    let
      class = unitClasses.${className};
    in
    lib.concatMap (
      unitName:
      validateUnit class deviceRequirements.unitSignatures.${className}.${unitName} name device unitName
    ) deviceRequirements.requiredUnits.${className};
  validateSystemdPackages =
    name: device: deviceRequirements:
    let
      actual = map toString ((device.config or { }).systemd.packages or [ ]);
    in
    lib.concatMap (
      package: lib.optional (!(builtins.elem package actual)) (systemdPackageFailure name package)
    ) deviceRequirements.requiredSystemdPackages;
  validateModule =
    name: device:
    let
      markerPath = [
        "services"
        "korriProduct"
        "installed"
      ];
      markerDeclared = lib.hasAttrByPath markerPath (device.options or { });
      markerEnabled = valueAt markerPath false (device.config or { });
    in
    # This marker improves diagnostics only. Nix module metadata is forgeable;
    # the gate's guarantee is the complete behavioral contract below.
    lib.optional (!(markerDeclared && markerEnabled)) (missingModuleFailure name);

  validate =
    name: device:
    let
      deviceRequirements = requirementsForDevice device;
      settingFailures = lib.concatMap (validateSetting name device) deviceRequirements.settings;
      # A forced parent setting can make dependent unit implementations
      # intentionally unevaluable. Report that public-contract failure first;
      # unit liveness has its own final-value cases once settings match.
      unitFailures = lib.optionals (settingFailures == [ ]) (
        lib.concatMap (
          className: validateUnitClass className name device deviceRequirements
        ) (builtins.attrNames unitClasses)
        ++ validateSystemdPackages name device deviceRequirements
      );
    in
    validateModule name device ++ settingFailures ++ unitFailures;

  validateAll =
    configurations:
    lib.filterAttrs (_: failures: failures != [ ]) (lib.mapAttrs validate configurations);

  formatFailures =
    failures:
    lib.concatStringsSep "\n" (
      lib.concatMap (name: map (failure: "- ${failure}") failures.${name}) (builtins.attrNames failures)
    );
in
{
  inherit
    activeUnitNames
    activationFields
    formatFailures
    missingModuleFailure
    requiredUnits
    requirements
    requirementsForSystem
    settingFailure
    settingMatches
    systemdPackageFailure
    systemServiceFailure
    systemServiceLivenessFailure
    systemSocketFailure
    systemSocketLivenessFailure
    unitClasses
    userServiceFailure
    userServiceLivenessFailure
    userSocketFailure
    userSocketLivenessFailure
    validate
    validateAll
    validateModule
    validateSetting
    ;
}
