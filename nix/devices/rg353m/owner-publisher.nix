# The owner's personal plugin publisher, trusted on this RG353M only.
#
# Owner decision 2026-10-28: every handheld runs `main` and keeps its installed
# `@simonwjackson` plugins through the normal plugin host, with signature,
# publisher-binding and exact-approval checks in force. haku has
# `@simonwjackson:eggshell` and `@simonwjackson:eggshell-rust` installed and
# enabled (2026-11-05).
#
# Grounding: the binding shape is `services.korri.pluginHost.publishers`, as in
# nix/devices/rg35xxpro/owner-publisher.nix. The key and the root-only file
# cache are the ones haku's running generation 39 binds
# (/etc/korri-plugin-host/publishers.json, read 2026-11-05).
{
  services.korri.pluginHost.publishers."@simonwjackson" = {
    publicKey = "simonwjackson-plugins-1:4SchQgTMNU3syZ2ZzN/cJXPMigc/PIt+EaqHAStgJs8=";
    cacheUrl = "file:///var/lib/eggshell-profile/cache";
  };
}
