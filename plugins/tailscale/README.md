# Tailscale for Linux

The owner installs Tailscale separately. The system image must not contain its
program files. Later Tailscale updates must not require a system update or
restart korrid.

This directory currently provides a package check. The flake exports the native
Tailscale package as `packages.<system>.korri-tailscale` on Linux. Plugin
installation, service integration, and rollback are not implemented yet.

## Package ownership

`korri-tailscale` is the unchanged `pkgs.tailscale` package from `flake.lock`.
Nixpkgs supplies `bin/tailscale`, `bin/tailscaled`, their runtime dependencies,
and the vendor systemd unit. No wrapper package or separate source pin is
necessary. Exporting the package does not install it on a device.

Builds run on development or CI machines. Devices receive prebuilt Nix store
files and their dependencies from a trusted cache or an offline package. A
missing prebuilt dependency must stop installation, not trigger compilation
on the device. Cache configuration belongs to the separate Garnix work.

This native package is a dependency, not executable Korri plugin logic.
[`services/korrid/SCRIPTING.md`](../../services/korrid/SCRIPTING.md) defines
source-only declarations. Its contribution kinds do not cover this service.
There is no `plugin.ts` here and no new declaration kind.

## Checks

Run on a development or CI machine that can execute the selected architecture:

```sh
nix build .#korri-tailscale --no-link
nix build .#checks.x86_64-linux.korri-tailscale-package --no-link
nix build .#checks.aarch64-linux.korri-tailscale-package --no-link
```

The check runs both executables and compares their reported versions with the
package version. It also requires the CLI to fail against an absent daemon
socket. It does not start a daemon, use a device's login state, or join a tailnet.
These checks prove executable behavior only. They do not prove networking or
plugin installation.

## Service integration still required

Installation must work on a device with no Tailscale-specific service or package
registration in its system configuration. Generic installation support and
required kernel capabilities can exist beforehand. The owner approved installation
of trusted system software. The [installation brief](../../docs/briefs/2026-09-08-linux-plugin-installation-brief.md)
records that decision, an isolated portable-service VM test, and the remaining
implementation requirements.

The existing core bundle initializer requires an initial package. Do not reuse
that requirement for Tailscale. Do not add the earlier proposed per-plugin
NixOS registration: the owner installs a plugin the image does not know.

The Tailscale integration must meet these requirements:

- Generic installation support obtains explicit owner authorization to install
  the service and its prebuilt program files. Native payloads can receive
  administrator-level authority. Korri does not promise browser-style isolation.
- The daemon and every CLI consumer use the selected Tailscale version.
  Nixpkgs uses `services.tailscale.package` for the vendor unit, the system CLI,
  autoconnect, and the set helper. A daemon-only override is insufficient.
- An absent or disabled installation starts no Tailscale daemon or helper.
  Disabling a running installation stops its services. A systemd condition
  alone does not stop a running service.
- The vendor unit's arguments, cleanup, and state handling remain intact.
  Login state survives disablement. Rollback needs a separate state-compatibility
  check because older binaries can reject newer state.
- Peer reachability does not add Tailscale detection to korrid. The existing
  `services.korriLinuxHost.firewallInterfaces` option is a reference for firewall
  policy, not a runtime installation mechanism. Firewall changes and stream
  reachability need verification without a system update.

The stock NixOS module retains its package through `environment.systemPackages`
and `systemd.packages`. Enabling it unchanged violates the separate-install
requirement. Runtime service installation must address this before device wiring.

No plugin state paths, restart-policy format, install commands, auth-key path,
or signature exception are selected here. Those contracts remain unresolved.
Android, device enrollment, and Zao configuration changes are outside this work.
