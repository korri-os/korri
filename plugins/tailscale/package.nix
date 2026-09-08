{ pkgs }:
pkgs.runCommand "korri-tailscale-${pkgs.tailscale.version}" { inherit (pkgs.tailscale) version; } ''
  mkdir -p "$out/bin"
  cp ${./plugin.ts} "$out/plugin.ts"
  ln -s ${pkgs.tailscale}/bin/tailscaled "$out/bin/tailscaled"
  ln -s ${pkgs.tailscale}/bin/tailscale "$out/bin/tailscale"
''
