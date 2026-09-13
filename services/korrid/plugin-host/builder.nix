# Build-side public API. plugin.nix owns native NixOS service configuration;
# trusted publisher composition supplies identity. Devices receive only source,
# generated metadata and native artifacts, never a NixOS module to evaluate.
{ pkgs }:
{
  publisher,
  source,
  plugin,
}:
let
  lib = pkgs.lib;
  definition =
    if builtins.isFunction plugin then plugin { inherit pkgs; } else import plugin { inherit pkgs; };
  build =
    {
      packages ? { },
      files ? { },
      services ? { },
      requires ? [ ],
      ports ? { },
    }:
    let
      isNative =
        service: builtins.isPath service || builtins.isString service || lib.isDerivation service;
      nativeServices = lib.filterAttrs (_: isNative) services;
      renderedServices = lib.filterAttrs (_: service: !isNative service) services;
      evaluated = import (pkgs.path + "/nixos/lib/eval-config.nix") {
        inherit pkgs;
        system = pkgs.stdenv.hostPlatform.system;
        modules = [
          {
            # A package is not the CI machine's NixOS system. Do not inherit its
            # locale, timezone or default command search path. Explicit plugin
            # environment directives still render and the host refuses them.
            systemd.globalEnvironment = lib.mkForce { };
            systemd.services = lib.mapAttrs (
              _: service:
              lib.mkMerge [
                service
                { path = lib.mkForce [ ]; }
              ]
            ) renderedServices;
          }
        ];
      };
      units =
        lib.mapAttrs (_: toString) nativeServices
        // lib.mapAttrs (
          name: _: "${evaluated.config.systemd.units."${name}.service".unit}/${name}.service"
        ) renderedServices;
      # Schema grounding: approved plugin.nix fields, publisher.namespace from
      # the existing signed manifest, and NixOS's firewall protocol lists.
      manifest = pkgs.writeText "plugin-manifest.json" (
        builtins.toJSON {
          inherit
            publisher
            files
            requires
            ports
            ;
          packages = lib.mapAttrs (_: toString) packages;
          services = units;
        }
      );
      validName =
        name: builtins.match "[a-z0-9][a-z0-9_.-]*" name != null && builtins.stringLength name <= 64;
      searchPaths = builtins.filter (name: renderedServices.${name} ? path) (
        builtins.attrNames renderedServices
      );
      # The host's immutable_path rule: a file is a path inside a store output,
      # never the output itself. A derivation whose $out is the file passes
      # `test -e` here and is then refused on the device at inspection, which
      # is where RetroArch's settings evidence was first caught. Refuse it at
      # build instead, where the author can act on it.
      bareOutputs = builtins.filter (
        name: builtins.match "/nix/store/[^/]+" (toString files.${name}) != null
      ) (builtins.attrNames files);
    in
    assert lib.assertMsg (bareOutputs == [ ])
      "plugin files ${builtins.concatStringsSep ", " bareOutputs}: a file must be a path inside a store output, not the output itself";
    assert lib.assertMsg (searchPaths == [ ])
      "plugin services ${builtins.concatStringsSep ", " searchPaths}: path is unsupported; use immutable ExecStart paths";
    assert builtins.all validName (
      builtins.attrNames packages ++ builtins.attrNames files ++ builtins.attrNames services
    );
    assert builtins.attrNames publisher == [ "namespace" ];
    pkgs.runCommand "korri-plugin" { } ''
      mkdir -p "$out"
      cp ${source} "$out/plugin.ts"
      cp ${manifest} "$out/manifest.json"
      ${lib.concatMapStringsSep "\n" (path: "test -e ${lib.escapeShellArg (toString path)}") (
        builtins.attrValues files
      )}
      ${lib.concatMapStringsSep "\n" (path: "test -f ${lib.escapeShellArg path}") (
        builtins.attrValues units
      )}
    '';
in
build definition
