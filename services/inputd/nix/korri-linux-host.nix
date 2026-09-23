# Product-agnostic Linux compositor, input, audio, and korrid substrate.
# Removable integrations such as Sunshine ship through the plugin host.
{ korri }:
{
  imports = [ (import ./korri-linux-host-core.nix { inherit korri; }) ];
}
