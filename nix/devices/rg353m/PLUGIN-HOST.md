# RG353M plugin host

The normal and rescue images enable the generic plugin host. They include no
installed or enabled plugin inventory. The integration is a deliberate port of
`a8994384:nix/rg353m/plugin-host.nix`, using the current native
[`services.korri.pluginHost` module](../../../services/korrid/plugin-host/nixos-module.nix).
No plugin declaration, receipt schema, or migration is added.

The approved publisher binding uses the existing `korri-os/plugins` GitHub
`NIX_CACHE_PUBLIC_KEY` variable, verified for this integration, not a generated
key:

| Native option | Value |
|---|---|
| `publishers."@korri".publicKey` | `korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98=` |
| `publishers."@korri".cacheUrl` | `https://github.com/korri-os/plugins/releases/download/cache/` |

This binding authenticates publisher bytes. It does not approve installation,
activation, or root authority. Each plugin still needs explicit approval.
The URL is a signed Nix cache, not an HTTPS catalog. `officialCatalogUrl`
remains unset. The plugin cache is not a general device-generation substituter.

The native host supplies the CLI, boot `restore-all`, empty state/runtime/GC
root directories, TUN support, and OpenSSH account/PAM prerequisites. It does
not start a listener on TCP 22 or 2222, generate SSH host keys, enroll root, or
add personal credentials. The shared release policy still disables OpenSSH.
The existing USB interface firewall allowance for TCP 22 is unchanged; an
allowance is not a listener. Physical-console root access is also unchanged.

## Build-machine checks

```sh
nix build --no-link .#checks.x86_64-linux.rg353m-plugin-host
nix build --no-link .#checks.x86_64-linux.korri-base
```

The focused check evaluates both real image configurations. It checks host
bootstrap, publisher binding, download-only settings, SSH-disabled release
policy, and absence of seeded plugin inventory. It does not build ARM software,
run a VM, prove live cache delivery, or validate a physical device.

## One-time host update boundary

The hostname remains `haku`; the public flake attribute remains `rg353m`.
The generic generation build attribute is
`nixosConfigurations.rg353m.config.system.build.toplevel`. The normal image
attribute remains `packages.aarch64-linux.rg353m-sd-image`.
Build only on a build machine. Follow the
[device-cache policy](../../device-cache/README.md) for prebuilt delivery.
The enabled policy refuses local builds, remote builders, unsigned
input-addressed downloads, and source fallback. Missing core outputs still
block an update.

Do not activate the generic release configuration over an existing recovery SSH
connection: it disables that listener. A separately reviewed private deployment
overlay must preserve existing SSH on TCP 22 and the active game bundle for the
one-time compatible host update. This slice supplies no such overlay, deploys
nothing, switches no device, and makes no plugin selections. It also supplies
no receipt migration. Compatibility, prebuilt delivery, and physical acceptance
remain deployment gates owned by that separate operation.
