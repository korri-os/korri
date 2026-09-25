{
  pkgs,
  plugins,
  korri,
  devices,
}:
let
  lib = pkgs.lib;
  published = (import (plugins.outPath + "/nix") { inherit korri; }).packages.aarch64-linux;
  selected = import ./plugin-selection.nix {
    plugins = published;
  };
  names = [ "rg353m" "rgds" "r36tmax" "rpminiv2" "odin2portal" ];
  exactImage = name:
    let
      config = devices.${name}.configuration.config;
      image = config.sdImage;
      proofs = config.systemd.services.korri-plugin-offline-proofs;
      additional = lib.subtractLists [ (toString config.system.build.toplevel) ] (map toString image.storePaths);
    in
    additional == map toString selected.${name}
    && lib.hasInfix "korri-image-seed" image.populateRootCommands
    && (
      name == "r36tmax"
      || (proofs.requiredBy == [ "korri-plugin-host.service" ]
        && lib.elem "korri-plugin-host.service" proofs.before
        && lib.hasInfix "/bin/gzip -dc" proofs.script
        && lib.hasInfix "--option substituters \"\" store copy-sigs" proofs.script)
    )
    && lib.all (package: !(lib.elem (toString package) (map toString config.environment.systemPackages))) selected.${name};
  recoveryWithoutDefaults =
    !(lib.hasInfix "korri-image-seed" devices.r36tmax.consoleConfiguration.config.sdImage.populateRootCommands)
    && !(lib.hasInfix "korri-image-seed" devices.rpminiv2.consoleConfiguration.config.sdImage.populateRootCommands);
in
assert lib.all exactImage names;
assert recoveryWithoutDefaults;
assert !(lib.elem (toString published.korri-plugin-sunshine) (map toString selected.r36tmax));
assert lib.all (name: lib.elem (toString published.korri-plugin-sunshine) (map toString selected.${name}))
  [ "rg353m" "rgds" "rpminiv2" "odin2portal" ];
pkgs.runCommand "korri-product-image-plugins-check" { } ''
  touch "$out"
''
