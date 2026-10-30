# Build-host preparation reads facts without forcing plugin store contexts.
{
  pkgs,
  system,
  device,
}:
let
  lib = pkgs.lib;
  catalog = import ./catalog-plugins.nix {
    inherit pkgs system;
    withContext = false;
  };
  selection = import ./plugin-selection.nix { plugins = catalog.records; };
  selected =
    if device == "--all" then
      builtins.attrValues catalog.records
    else if device == "--inputd" then
      [ catalog.records.korri-plugin-sunshine ]
    else
      selection.${device};
  names = map (record: "korri-plugin-${lib.removePrefix "@korri:" record.plugin_id}") selected;
in
{
  inherit (catalog) catalog catalogUrl;
  records = lib.filterAttrs (name: _: lib.elem name names) catalog.records;
  archives = lib.filterAttrs (name: _: lib.elem name names) catalog.archives;
}
