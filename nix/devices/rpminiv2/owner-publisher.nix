# The owner's personal plugin publisher, trusted on this Mini V2 only.
#
# Owner decision 2026-10-21: the Mini V2 keeps installing `@simonwjackson`
# plugins (Fallout CE, OpenGOAL, 2Ship, ZQuest, Mega Man Maker and others)
# through the normal plugin host, with signature, publisher-binding and
# exact-approval checks in force.
#
# Grounding: the binding shape is `services.korri.pluginHost.publishers`
# (services/korrid/plugin-host/nixos-module.nix), as in
# nix/devices/odin2portal/owner-publisher.nix. The key and the root-only file
# cache are the ones the device's previous generation bound
# (/etc/korri-plugin-host/publishers.json); its installed @simonwjackson
# receipts name that cache.
{
  services.korri.pluginHost.publishers."@simonwjackson" = {
    publicKey = "simonwjackson-plugins-1:4SchQgTMNU3syZ2ZzN/cJXPMigc/PIt+EaqHAStgJs8=";
    cacheUrl = "file:///var/lib/korri-private-cache.h6odJOu2/cache";
  };
}
