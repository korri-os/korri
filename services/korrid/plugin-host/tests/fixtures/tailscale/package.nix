{ pkgs }:
# Host acceptance fixture, not a distributable plugin. Preserve the real
# Tailscale package/declaration moved from core to korri-os/plugins.
pkgs.runCommand "korri-plugin-host-test-tailscale-${pkgs.tailscale.version}" { } ''
  mkdir -p "$out/bin"
  cp ${./plugin.ts} "$out/plugin.ts"
  echo '${builtins.toJSON { publisher.namespace = "@korri"; }}' > "$out/manifest.json"
  ln -s ${pkgs.tailscale}/bin/tailscaled "$out/bin/tailscaled"
  ln -s ${pkgs.tailscale}/bin/tailscale "$out/bin/tailscale"
''
