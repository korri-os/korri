# Ported deliberately from a8994384:nix/rg353m/plugin-host.nix.
# The generic host restores only administrator-approved runtime selections;
# this image names no installed or enabled plugins. Its device-cache policy
# requires every generation to arrive prebuilt from a build machine.
{ korri, ... }:
{
  imports = [
    (import ../../../services/korrid/plugin-host/nixos-module.nix { inherit korri; })
  ];

  services.korri.pluginHost = {
    enable = true;
    # Existing korri-os/plugins GitHub NIX_CACHE_PUBLIC_KEY, verified for this
    # integration. This is publisher trust, not installation or root approval.
    publishers."@korri" = {
      publicKey = "korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98=";
      cacheUrl = "https://github.com/korri-os/plugins/releases/download/cache/";
    };
    # No officialCatalogUrl: a signed Nix cache is not an HTTPS catalog.
  };
}
