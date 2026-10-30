# The approved catalog is the only release pin. Preparation imports its archives
# with Nix on the build host before path contexts are evaluated. No plugin
# recipe, raw-cache fallback, or second release list enters the image lane.
{
  pkgs,
  system,
  withContext ? true,
}:
let
  lib = pkgs.lib;
  pin = import ./published-plugins.nix;
  catalog = pkgs.fetchurl {
    url = pin.catalogUrl;
    hash = pin.catalogSha256;
  };
  data = builtins.fromJSON (builtins.readFile catalog);
  matching = builtins.filter (
    record:
    (record.platform == system || record.platform == "any")
    && data.latest.${record.plugin_id} == record.release_version
  ) data.records;
  records = builtins.listToAttrs (
    map (record: {
      name = "korri-plugin-${lib.removePrefix "@korri:" record.plugin_id}";
      value = record;
    }) matching
  );
  plugins = builtins.mapAttrs (
    _: record:
    if withContext then
      builtins.appendContext record.store_path {
        "${record.store_path}".path = true;
      }
    else
      record.store_path
  ) records;
  archives = builtins.mapAttrs (
    _: record:
    pkgs.fetchurl {
      url = record.archive_url;
      sha256 = record.archive_sha256;
    }
  ) records;
in
assert lib.all (record: lib.hasPrefix "@korri:" record.plugin_id) data.records;
assert builtins.length matching == builtins.length (builtins.attrNames records);
{
  inherit
    catalog
    records
    plugins
    archives
    ;
  inherit (pin) catalogUrl;
}
