# Linux portal deployment

The device runs one local web server and one Chromium kiosk. The portal bundle
contains every registered surface. A web update replaces neither the operating
system nor the streaming services.

The device portal uses the same RPC client, token authentication and RPC
handlers as Android. Linux currently receives read-only permission. It reads
actual korrid responses. It does not launch games or change settings. An empty
device catalog stays empty. Unsupported native actions do not simulate success.
Browser development without a native credential binding still uses offline
fixtures.

## Install the runtime once

Import `nixosModules.korri-portal` alongside the existing Korri Linux host module.
Enable `services.korri.webSurfaceHost.enable` and
`services.korri.compositor.kiosk.enable`. The RG353M composition is available as
`nixosConfigurations.rg353m`.

The browser runs as `korri-portal`, separate from the untrusted game user. Its
private home is `/var/lib/korri-portal`. Its private runtime directory is
`/run/korri-portal`. It receives only the existing Wayland socket through a
private bind mount. Startup grants this user an ACL on that exact socket because
a bind mount preserves the compositor's mode 0700. The parent runtime directory
stays private. No input-group, raw-controller or inputd-control access is granted.
The server listens only on `127.0.0.1:8099`. It opens no firewall port.

A root-owned service generates a capability in its private runtime directory.
Systemd supplies separate private credential mounts to korrid and the browser
shell. The shell injects the shared `KorriRpc` binding through Chromium's private
control pipe, before loading the trusted page. The capability never enters the
web bundle, URL, command arguments or browser storage. There is no debugging TCP
listener and no RPC proxy. The browser calls korrid's existing `/rpc` directly.

The shell reports systemd readiness only after the trusted top-level page reads
both credential getters. This rejects old fixture bundles that ignore the
binding. Readiness does not prove that every RPC succeeds. Verify actual browser
data separately. Credential-service restart rotates the token and restarts both
consumers. A normal web update restarts only the browser.

The RG353M composition uses reduced motion and software browser drawing. Its
pinned Chromium GPU process crashes inside the sandbox, including with the
benchmark's GLES flags. `--disable-gpu` avoids that crash loop without weakening
the sandbox. This costs browser acceleration; it does not change Sway rendering
or Sunshine's hardware encoding. Larger or animated pages need separate testing.

There is no portal-specific temperature cutoff. The earlier 68°C guard was
removed at the user's request because it interrupted boot and web updates.
Linux's existing thermal management remains unchanged.

Build and install this runtime through the device's existing NixOS deployment
procedure. Preserve its hardware configuration. Test activation before changing
the persistent boot selection. The RG353M used for initial
acceptance has additional streaming changes on `feat/rg353m-rkvenc-mpp`; building
plain main is not a substitute for that working system. No raw disk or eMMC
write is part of portal deployment.

## Cut over an existing preview

This is an explicit deployment operation, not a runtime migration. Stop the old
browser before copying its profile. Keep the original profile and selected web
bundle as backups. After activation creates `korri-portal`, copy the preserved
profile under its home using the existing Chromium profile suffix. Set ownership
to `korri-portal:korri-portal` before starting the new browser.

Select a web bundle that implements `KorriRpc` and a service bundle containing
the updated korrid. Preserve both previous bundle selections and the previous OS
closure as GC roots. Old fixture web generations are not valid rollback targets
for the new shell. Keep them with the previous runtime for operational recovery,
not in the live web rollback sequence. Never silently restore sample data when
the real connection fails.

## Update only the web app

From the checkout containing the desired portal source:

```sh
nix run .#portal-deploy -- root@DEVICE
nix run .#portal-deploy -- root@DEVICE --status
nix run .#portal-deploy -- root@DEVICE --rollback
```

Use `--ssh-config FILE` for a specific SSH configuration. Host aliases work.
The account needs permission to change the portal profile and restart its
browser. The command does not install the runtime or change the system profile.

The update command builds `packages.<system>.korri-portal`, retains a temporary
local GC root, copies the immutable output over SSH, then invokes
`korri-portal-select` on the device. That copy explicitly trusts the operator's
local build with `--no-check-sigs`; it does not change the device's trust settings.
Only deploy source you trust. The output is static web content with no store
references, so a laptop's output can run on the ARM device.

Nix owns selection and history through its named profile
`/nix/var/nix/profiles/korri-portal`. Updates are serialized. The selector restarts
only Chromium, checks both service states, and compares the served index with
the selected bundle. Failed activation restores the original generation and
removes a newly created failed generation so a future rollback cannot select it.
Previously accepted generations remain available. These checks establish server
and process health, not that every UI interaction works.

The kiosk is wanted by its server and compositor as well as the boot target.
If either parent's initial start fails, its later start pulls the kiosk back in.
Stopping or restarting a parent also stops or restarts its kiosk.

Startup initializes the profile only when it is absent. Reboot and later NixOS
activation do not overwrite a valid web selection with the bundled initial
version. Do not manually delete profile generations still needed for rollback.

## Select a surface

The existing portal URL parameter selects a registered surface:
`http://127.0.0.1:8099/?surface=pico` or `?surface=shift`.
The portal remembers this non-sensitive preference in its browser profile.
Normal kiosk startup uses the bare URL, so the remembered choice survives
browser restart and reboot. No surface selector UI is added by this slice.

Adding a surface still requires registering it and including its source in the
portal build. It does not require another device server, kiosk, or update script.
Surfaces ship together in one bundle; independent surface downloads and version
negotiation are not implemented.

## Verification and recovery

```sh
nix run .#portal-runtime-check
nix run .#portal-check
nix run .#pico-check
nix run .#shift-check
nix build .#checks.x86_64-linux.korri-portal-module
nix build .#korri-portal
```

The selector tests use actual temporary Nix profiles, a real HTTP server and a
configured service subprocess. They cover startup preservation, update,
rollback, invalid bundles, failed restarts, stale HTTP and rejection history.
The module check evaluates actual NixOS units, including disabled and
non-loopback configurations, private credentials, user isolation and startup
ordering. Device acceptance must inspect the actual browser, token rejection,
read-only enforcement, Wayland access, streaming, reboot and rollback.

`nix/browser-check.py` is the historical fixture-preview check. It expects sample
games and no backend requests, so it is not the live-data acceptance gate. Do not
add a debugging port to the production kiosk to run it. The native shell tests
use actual sandboxed Chromium and its private pipe. The Android bridge gate uses
an isolated emulator. The older [RG353M acceptance](../../docs/acceptance/rg353m-portal-2026-09-06.md) remains evidence for the prior preview only.

If an update reports failure, inspect the selected generation and service logs.
A restored bundle does not repair an unavailable compositor.
For runtime rollback, use the device's retained NixOS generation and its boot
installer. Do not restore an old extlinux file whose referenced files were
pruned. Normal web updates do not touch extlinux.

## Grounding

`KorriRpc` extracts the existing `korridPort()` and `korridCapability()` methods
from the Android bridge treaty. Systemd's credential ID uses korrid's existing
`KORRID_RPC_CAPABILITY` name. The isolated user, home and runtime directory use
the existing `korri-portal` package namespace. No user library schema changes.

The names `services.korri.webSurfaceHost`, `services.korri.compositor.kiosk`,
loopback port 8099, and the URL/Chromium profile environment come from legacy's
`product/systems/nixos/modules/korri-web-surface-host.nix`,
`product/systems/nixos/modules/korri-compositor.nix`, and
`product/services/device/nix/chromium-kiosk.nix`. Only the preview-relevant
options are extracted. Nginx supplies static serving; the legacy API proxy and
sessiond are not ported.

The asset-root separation preserves legacy's `KORRI_ASSET_ROOT` boundary.
The named profile uses Nix's existing generation and GC-root format, named for
legacy's `korri-portal` package. It introduces no custom deployment manifest.
Browser preference storage is owned by `src/surface/surface-preference.ts`.
No separate portal thermal policy is installed.
