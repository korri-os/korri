# Tailscale for Linux

Tailscale installs independently through the [generic plugin host](../../services/korrid/plugin-host/README.md). The system image contains no Tailscale package or named service registration. Install, enable, update, disable, and remove use the administrator CLI without a NixOS update.

`packages.<system>.korri-tailscale` contains the runtime-interpreted `plugin.ts` declaration and links to the unchanged `pkgs.tailscale` binaries from `flake.lock`. The small declaration package is also built in CI. Devices fetch its signed closure from Garnix and never build it locally. Both CLI and daemon come from that same selected package.

## Installation

First install generic `nixosModules.korri-plugin-host` support in the base system. The module does not name Tailscale. Use the exact package path from the build for the device architecture.

```sh
sudo korri-plugin inspect https://cache.garnix.io "$PACKAGE"
```

Read the reported declaration, effective systemd policy, and permission warning. Then supply the report's exact approval value:

```sh
sudo korri-plugin install https://cache.garnix.io "$PACKAGE" "$APPROVAL"
sudo korri-plugin enable @korri:tailscale
```

The daemon uses a dynamic unprivileged user with `CAP_NET_ADMIN` and `CAP_NET_RAW`. These capabilities permit host-network changes, including routes and firewall rules. Approval grants substantial authority. It is not browser-style isolation or a promise that the publisher is safe.

The declaration uses the upstream daemon command, notification readiness, and cleanup operation. systemd supplies private state and socket directories. The host reports their exact paths. There are no install scripts, plugin-specific host options, or authentication keys in the package.

## Authentication and limits

Enablement starts the daemon. It does not join a tailnet. Use the selected package's CLI and the socket path from the report to perform a separately authorized login with `--accept-dns=false`. This first slice carries traffic without changing the host DNS configuration. Keep secrets out of declarations and the Nix store.

Private state survives disablement, updates, and ordinary removal. `remove --purge` explicitly deletes it. Automatic rollback restores package selection, not a snapshot of application data. A newer binary can change state that an older binary cannot read. Failed recovery retains the package references and reports an error.

The maintained VM proves TUN creation, authenticated IP traffic through a disposable local Headscale network, cleanup, reboot recovery, and the full installation lifecycle. It does not prove host DNS changes, handheld kernel support, or remote streaming. The restricted service cannot rewrite arbitrary host files or acquire new permissions.

## Checks

Run these on a development or CI machine, never on a download-only device:

```sh
nix run .#korri-plugin-check
nix build .#checks.x86_64-linux.korri-tailscale-package --no-link
```

Garnix's explicit build list includes the native package and generic host for both Linux architectures. Publication still requires the normal repository push and configured Garnix app. No device deployment or real tailnet enrollment is part of these checks.
