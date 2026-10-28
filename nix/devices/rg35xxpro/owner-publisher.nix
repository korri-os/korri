# The owner's personal plugin publisher, trusted on this RG35XX Pro only.
#
# Owner decision 2026-10-28: every handheld runs `main`, and the Pro keeps its
# installed `@simonwjackson` plugins (`@simonwjackson:eggshell`,
# `@simonwjackson:pogo-pogo`) through the normal plugin host, with signature,
# publisher-binding and exact-approval checks in force.
#
# Grounding: the binding shape is `services.korri.pluginHost.publishers`, as in
# nix/devices/rpminiv2/owner-publisher.nix and
# nix/devices/odin2portal/owner-publisher.nix. The key and the root-only file
# cache are the ones the Pro's previous generation bound
# (/etc/korri-plugin-host/publishers.json, generation 49).
{
  services.korri.pluginHost.publishers."@simonwjackson" = {
    publicKey = "simonwjackson-plugins-1:4SchQgTMNU3syZ2ZzN/cJXPMigc/PIt+EaqHAStgJs8=";
    cacheUrl = "file:///var/lib/eggshell-profile/cache";
  };
}
