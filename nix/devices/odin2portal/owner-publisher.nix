# The owner's personal plugin publisher, trusted on this Odin only.
#
# Owner decision 2026-10-21: the Odin installs `@simonwjackson` plugins (first
# case: `@simonwjackson:mega-man-arena`) through the normal plugin host, with
# signature, publisher-binding and exact-approval checks in force.
#
# Grounding: the binding shape is `services.korri.pluginHost.publishers`
# (services/korrid/plugin-host/nixos-module.nix); product/module-check.nix
# checks an added owner binding beside `@korri`. The public key is the one the
# owner's existing `@simonwjackson` binding on the Mini V2 uses. The cache is a
# root-only file cache on the device, as on the Mini V2: packages arrive signed
# by that key, and nothing is published.
{
  services.korri.pluginHost.publishers."@simonwjackson" = {
    publicKey = "simonwjackson-plugins-1:4SchQgTMNU3syZ2ZzN/cJXPMigc/PIt+EaqHAStgJs8=";
    cacheUrl = "file:///var/lib/korri-private-cache/cache";
  };
}
