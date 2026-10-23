# Linux WPE portal launcher

`korri-portal-wpe` is the native Linux client for the
[`KorriRpc` bridge](../../contracts/bridge/korri-rpc-bridge.ts).
It embeds the vendored `korri-wpewebkit` (currently 2.52.6) through WPEPlatform. It accepts no arguments.
There is no Chromium subprocess, debugging pipe, debugging listener or fallback.

## Startup inputs

| Environment input | Contract |
| --- | --- |
| `CREDENTIALS_DIRECTORY` | Absolute systemd credential directory. Read only `KORRID_RPC_CAPABILITY` inside it. |
| `KORRID_ADDRESS` | Numeric socket address with a nonzero port. Only the port enters the binding; the portal connects to `127.0.0.1`. |
| `KORRID_PORTAL_ORIGIN` | Canonical `http://127.0.0.1:PORT`, with a nonzero explicit port. |
| `KORRI_WEB_SURFACE_URL` | The same origin followed by `/` and an optional path/query. |
| `HOME`, `XDG_RUNTIME_DIR` | Required absolute paths belonging to the isolated portal service. |
| `WAYLAND_DISPLAY` | Required for Wayland. |
| `WPE_PLATFORM` | Optional `wayland` (default) or `headless`. The launcher builds that display itself and never falls back to another backend; a missing Wayland socket fails startup. Headless is for acceptance tests. |
| `NOTIFY_SOCKET` | Optional filesystem or `@`-prefixed abstract Unix datagram socket. |

The credential must be a regular file in one of two shapes. Either it is owned
by the effective service user with no group or other permission bits, or it
has the form systemd's `LoadCredential=` produces for a non-root service:
owned by root, and readable by the service user through a POSIX ACL entry,
with no other user or group able to read it. The launcher opens it with
`O_NOFOLLOW`, `O_CLOEXEC` and `O_NONBLOCK`. It rejects symlinks, empty files,
files larger than 4096 bytes, NULs and control characters. One trailing newline
is accepted. The value must use ASCII bearer-token characters
(`A-Z`, `a-z`, `0-9`, `-._~+/=`). It never reads the capability environment value.

Before creating WPE processes, the launcher removes all environment variables
except `HOME`, `XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY`, `WPE_PLATFORM`, `LANG`,
`LC_ALL`, `TZ`, `GBM_BACKENDS_PATH`, `LIBGL_DRIVERS_PATH` and
`__EGL_VENDOR_LIBRARY_FILENAMES`. The last three are native graphics-loader
inputs; the test app supplies store paths because Fuji has no
`/run/opengl-driver`. In particular, credential paths, `NOTIFY_SOCKET`, bus
addresses, `LD_*`, `WEBKIT_*`, inspector and sandbox overrides are not passed
to engine processes. The service must supply a trusted launcher environment:
variables such as `LD_PRELOAD` can act before native `main()` runs.

## Credential and readiness boundary

A document-start user script runs in the top frame only. WebKit's URL match
patterns do not support ports, so its allow list restricts injection to
`http://127.0.0.1/*`; the script's first statement checks the **exact** configured
`location.origin`, including its port, before installing any binding.

`window.KorriRpc` is non-enumerable, non-writable and non-configurable. Its
object is frozen. `korridPort()` returns a number and `korridCapability()` returns
a string. The binding is not written to a URL, command argument, static asset,
browser storage or log. Same-origin scripts share the trusted document's
security principal; this is not isolation from malicious code on that origin.
Child frames receive no binding of their own.

The script captures its native message sender before page code runs. After
both methods are consumed, it sends a 256-bit per-launch nonce through the
`korriReady` script-message handler. The nonce exists only in native memory and
the injected closure, not in page properties, environment or requests. The
launcher ignores wrong nonce messages. A page that does not consume both
methods fails after the production 15-second bootstrap deadline. Frame messages
without the nonce cannot make the service ready.

Only then does the main process send `READY=1` to `NOTIFY_SOCKET`. Notification
failure fails startup. Without a notification socket, it still requires binding
consumption. This proves binding consumption, not successful RPC or korrid
availability. Use `Type=notify`, `NotifyAccess=main`, `KillMode=control-group` and
a systemd startup timeout longer than 15 seconds.

The launcher stops loading and terminates its web process on failure or
SIGTERM/SIGINT. WPE owns its helper processes. Systemd must retain control-group
cleanup for helpers and for uncatchable signals. Run the portal as a separate
service user from games. The credential mount remains accessible to native
code with the same user identity; this launcher does not replace OS isolation.

## Browser policy

- Deny every `permission-request` with `webkit_permission_request_deny()`.
- Reject navigation outside the configured origin, including frame navigation.
- Reject every new-window policy decision and every view creation request.
- Disable developer extras, automatic JavaScript windows and console forwarding.
- Keep the engine's process sandbox enabled. No crash reporter is installed or
  started. Core dumps are disabled before WPE starts.
- Request fullscreen through `wpe_toplevel_fullscreen()`.
- Use persistent WPE network-session defaults under `HOME`. WPE owns its native
  data/cache layout; there is no Chromium profile reader or runtime migration.
  Any transfer of existing browser data is a separate deployment operation.

### Wayland app_id

The actual app_id is **`org.korri.portal`**. The tested WPE 2.52 API has no public app_id
setter. It obtains its app_id from `GApplication`, whose valid IDs require a
dot, so it cannot use the requested `korri-portal` through that API. The launcher
sets a non-registered, non-unique `GApplication` with `org.korri.portal` as its
ID. The saved Wayland test verifies the actual `xdg_toplevel.set_app_id` and
`set_fullscreen` protocol requests against a private headless Sway compositor.
Focus rules must match `org.korri.portal`.

### D-Bus decision

**No session bus.** The launcher removes inherited D-Bus addresses and does not
register its `GApplication`. Real-engine bootstrap, permissions and navigation
tests pass without a session bus. WPE's sandbox remains enabled. A private bus
and `dbus-run-session` are not needed for this slice. An accessibility-bus
warning can occur; accessibility integration is not verified here. The
launcher remains the unit's main process.

### Gameplay transparency

The native background is always transparent, with no opaque-region hint.
There is no `KORRI_PORTAL_TRANSPARENT` input. This follows the existing portal:
`src/main.tsx` switches between catalog and gameplay overlay in one window;
`src/index.css` paints catalog black and changes the overlay root to transparent.
A process-wide opt-in would prevent that in-place switch. The native WPE
background-color API supplies alpha without a browser patch. This does not
imply click-through. Compositor pixel blending and handheld GPU performance
still need device acceptance; the headless security tests do not prove them.

## Verification

```sh
nix build .#korri-portal-wpe
nix run .#korri-portal-wpe-test -- -v
```

The app runs the real launcher and engine with `WPE_PLATFORM=headless`, real
loopback HTTP servers, real credential files, and filesystem/abstract readiness
sockets. It checks bridge descriptors and values, console credential exclusion,
child binding exclusion, wrong nonces, full and partial bootstrap deadlines,
permission denial, popup rejection, off-origin navigation rejection, and
scrubbing of the engine process environment. Another origin is blocked before
its document can load or receive a binding.

For the temporary prebuilt aarch64 engine on Fuji, copy `clients/linux/` and
`flake.lock` into the same relative layout, then run the saved scripts there:

```sh
clients/linux/tests/fuji-build.sh
clients/linux/tests/fuji-test.sh -v
clients/linux/tests/fuji-wayland.sh
```

These scripts build locally on Fuji, not on handhelds. The build uses the
repository's locked nixpkgs and the two prebuilt WPE 2.52.3 store paths named in
`fuji-build.sh`. That engine lacks the separate DRM-node patch. The Wayland
protocol test can pass even if Fuji's GBM device cannot render a mapped window;
it reports the mapped-window count separately. It does not prove pixel output.

Focused formatting/lint checks:

```sh
nix shell nixpkgs#clang-tools --command clang-format -style=WebKit --dry-run --Werror clients/linux/src/main.c
nix shell nixpkgs#ruff --command ruff check clients/linux/tests/acceptance.py
nix shell nixpkgs#nixfmt --command nixfmt --check clients/linux/package.nix clients/linux/test-app.nix
```

The package builds C11 with `-Wall -Wextra -Werror` and hardening enabled.
