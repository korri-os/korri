# Steam's name for the Proton-CachyOS tool: the compatibilitytools.d
# directory, the compatibilitytool.vdf entry and the CompatToolMapping value.
# The single Nix definition; plugin.nix and the Steam plugin substitute it.
# It embeds the artifact version pinned in plugin.ts: the two must move
# together. No URL, hash or version pin lives here.
{
  toolName = "proton-cachyos-11.0-20260601-slr-arm64";
}
