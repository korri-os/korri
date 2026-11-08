# Complete shared Korri product composition: the server part and the client
# part (ADR 0051). Device modules import this once and provide hardware facts
# and recorded limits only. A device that only serves peers imports
# ./server.nix alone.
{ korri }:
{
  imports = [
    (import ./server.nix { inherit korri; })
    (import ./client.nix { inherit korri; })
  ];
}
