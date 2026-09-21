# Temporary native streaming composition. The shared host substrate is also
# used independently by the product; Sunshine-specific authority stays here.
{ korri }:
{
  imports = [
    (import ./korri-linux-host-core.nix { inherit korri; })
    (import ./korri-streaming-host.nix { inherit korri; })
  ];
}
