# Linux portal shell

`korri-portal-shell` starts Chromium and supplies the existing `window.KorriRpc`
bridge before the trusted portal loads. It implements
[`contracts/bridge/korri-rpc-bridge.ts`](../../contracts/bridge/korri-rpc-bridge.ts).
It does not implement RPC, hardware operations, or an Android bridge.

## Startup contract

```text
korri-portal-shell CHROMIUM [--flag ...]
```

`CHROMIUM` is an executable path. Each subsequent argument is one complete
Chromium flag. Use `--name=value`, not separate flag and value arguments. The
operator must supply trusted flags, including a private `--user-data-dir=...`.
The shell supplies the initial `about:blank` page and `--remote-debugging-pipe`.
It rejects caller-supplied URLs, debugging flags, app-mode navigation, session
restoration, and common sandbox-disabling flags. It never opens a debugging TCP
port. The caller must not wrap Chromium with a program that adds debugging or
sandbox-disabling flags.

| Input | Existing producer or consumer |
| --- | --- |
| `CREDENTIALS_DIRECTORY/KORRID_RPC_CAPABILITY` | systemd `LoadCredential`, with the existing korrid capability environment name as its credential ID. |
| `KORRID_ADDRESS` | korrid's existing socket address. The bridge uses only its nonzero port and the portal client connects to `127.0.0.1`. |
| `KORRID_PORTAL_ORIGIN` | korrid's existing allowed portal origin. This shell accepts the deployed Linux form, canonical `http://127.0.0.1:PORT`. |
| `KORRI_WEB_SURFACE_URL` | the existing Linux kiosk URL. It must start with the configured origin and `/`. Query parameters remain supported. |

The credential must be a regular file owned by the shell's service user, without
permissions for its group or other users. Symlinks and credentials above 4 KiB
are rejected. One final newline is accepted from the credential producer. The
shell does not read `KORRID_RPC_CAPABILITY` from its environment.

The caller also supplies normal Chromium environment values, including `HOME`,
`XDG_RUNTIME_DIR` and `WAYLAND_DISPLAY`. The shell removes the credential
variables and common Chromium flag/log overrides from the child environment.
It suppresses Chromium stdout and stderr. Shell failures report fixed messages
without protocol payloads or credentials. Core dumps are disabled. Chromium
receives `--disable-breakpad` and `--disable-crash-reporter`; caller flags cannot
enable crash reporting or file logging.

## Credential boundary

The shell uses two Unix socket pairs as Chromium's inherited fd 3 and fd 4.
Descriptor reservation prevents a `dup2` collision. The child starts in a new
process group. The shell attaches the initial page, installs the bridge, and
then navigates to the configured URL.

Startup succeeds only after the trusted top-level page calls both
`korridPort()` and `korridCapability()`. The shell receives this signal through
the private debugging pipe. It checks Chromium's execution-context origin,
frame and session before accepting the signal. A direct callback from a child
frame or another origin cannot report readiness. A page that ignores the bridge,
including an old fixture bundle, fails startup after 15 seconds. The shell then
stops and reaps Chromium.

After binding consumption, the shell sends `READY=1` to systemd's
`NOTIFY_SOCKET`. Configure `Type=notify`, `NotifyAccess=main` and a startup timeout
longer than the bootstrap deadline. The shell supports filesystem and abstract
Unix notification sockets. It removes `NOTIFY_SOCKET` from Chromium's environment.
Without this variable, standalone invocation still waits for binding consumption.
Notification failure fails startup.

This check proves that the page consumes the real connection binding. It does
not prove that korrid is available or that an RPC succeeds. Backend failures
remain the portal client's responsibility. No token appears in the readiness
signal, and no readiness HTTP endpoint or marker file is used.

The injected script installs the bridge only in the top-level document with the
exact configured origin. The binding is frozen, non-enumerable, non-writable,
and non-configurable. Navigation to another origin does not receive the binding.
Popups and child frames do not receive their own binding. Credentials never
enter browser URLs, command arguments, browser storage, or the public bundle.

Each protocol message has a 1 MiB limit. Startup commands have a 15-second limit.
The shell keeps the private debugging pipe open to monitor Chromium. It stops
the browser process group and reaps Chromium on failure or shutdown. SIGTERM and
SIGINT trigger this cleanup. Systemd must retain `KillMode=control-group` for
cleanup after an uncatchable signal such as SIGKILL.

Run this shell and Chromium under a service user separate from game processes.
Give that service only the Wayland access it needs. A credential mount still
exists inside the service, even though the child no longer receives its path
in the environment. This shell does not protect credentials from arbitrary
native code running under the same service user. It does not set up those OS
permissions or supply credentials to the service.

## Verification

```sh
cargo test --locked
KORRI_TEST_CHROMIUM=/absolute/path/to/chromium \
  cargo test --locked --test chromium -- --include-ignored
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The browser acceptance test uses an actual headless Chromium with its sandbox
enabled and two actual loopback HTTP servers. It checks authenticated use on
the trusted origin, absence on another origin, bridge properties, command-line
and environment exclusions, a real Unix readiness notification, and browser
cleanup. Four additional real-browser tests use the production 15-second
deadline. They reject an old fixture page, partial binding consumption, and
readiness callbacks from a child frame or an untrusted origin. Browser tests
require an explicit Chromium executable and are ignored during the default test
run. A default unit test verifies abstract Unix readiness notification.

## Chromium transparency experiment

[`chromium/`](chromium/) owns the independent native app-window transparency
patch, patched browser packages, and compositor pixel test. The experiment does
not change this shell's credential contract or certify the Odin GPU.
