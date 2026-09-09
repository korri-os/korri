# Private Chromium runtime delivery

This crate implements only the Linux kiosk launch and private runtime response.
It does not complete the web session or deploy anything.

`korri-kiosk` starts one Chromium process with `--remote-debugging-pipe`.
Chromium receives commands on FD 3 and sends NUL-terminated CDP JSON on FD 4.
There is no debugging TCP port or HTTP credential endpoint.

| Mode | Initial page |
| --- | --- |
| Native | `--app=http://127.0.0.1:8099/kiosk-blank.html` opens a nonfullscreen app window. Chromium 143 opens a normal tabbed Wayland window with `--app=about:blank`. |
| Headless | `--headless=new about:blank` avoids Chromium 143's ignored app flag and resulting new-tab page. |

The native bootstrap is the immutable, empty `clients/portal/public/kiosk-blank.html`
from the static portal package. Its CSP denies scripts, resource loads, connections,
forms and base-URL changes. It has no data, configuration or capabilities.
Before attaching, native startup polls for the exact bootstrap URL, permitting
only transient `about:blank` for up to 10 seconds; each CDP command also has its
own 10-second deadline. Unrelated initial URLs and multiple pages fail closed.
After attachment and Page enablement, the launcher rechecks the committed top
frame URL (and native security origin), enables Fetch interception, then privately
navigates to exactly `http://127.0.0.1:8099/`. Headless startup checks `about:blank`
instead. Neither initial page is trusted to receive credentials.

Fetch intercepts only `http://127.0.0.1:8099/runtime.json`. An eligible GET must
be Fetch/XHR from that page's top frame. The launcher checks both navigation
events and a fresh `Page.getFrameTree` response, including its exact URL and
security origin. Child frames and document navigations cannot receive the
response. Navigation away permanently revokes delivery in that browser session,
even if it later returns. A reload of the exact portal URL is allowed. Query
strings, fragments, alternate hosts and other portal paths are not trusted.
The normal static server must return 404 for runtime.json.

For each authorized request, the launcher reopens
`/run/korrid-browser/brain.json`. The existing producer is
`services/korrid/src/main.rs::publish_browser_runtime`; the response validator
follows `clients/portal/src/runtime-config.ts`. Only `korridPort` and
`korridCapability` are copied. An optional launch-time surface selection adds the
consumer's existing `surfaceId`. No new persisted configuration is introduced.
Atomic brain-file replacement therefore affects the next request; this does not
make an already-mounted portal reconnect without a reload.

## Launch-only implementation options

| Option | Meaning |
| --- | --- |
| `--chromium /absolute/executable` | Required Chromium executable or trusted package wrapper. No arbitrary extra Chromium arguments are accepted. |
| `--profile-parent /absolute/directory` | Required existing directory owned by the launcher UID, with no group/other permissions and no final-component symlink. |
| `--brain-file /absolute/file` | Override the existing brain-file path, primarily for isolated integration tests. |
| `--surface-id pico` | Optional nonempty surface preference, passed only in the private response. |
| `--headless` | Use Chromium's headless mode for the integration gate. The browser sandbox stays enabled. |

The parent directory is flocked for this launch. Chromium gets a new 0700
`chromium-<launcher-pid>` child directory, removed on normal/error teardown.
These are transient Chromium implementation files, not a Korri data schema.
Under that lock, startup removes stale Chromium profile children. It preserves
unrelated entries and rejects matching symlinks and non-directories. The
supervisor must stop old browser processes before starting another launcher.
The parent itself remains provisioned across kiosk stops. Every game must hide
it before the launcher starts; a missing optional path is not sufficient.

No capability is put in arguments, environment variables, diagnostics, or Nix
outputs. Chromium stdout/stderr go to `/dev/null`; launcher errors use fixed,
redacted classes. Core dumps are disabled; the launcher is non-dumpable. CDP
frames are limited to 1 MiB, the brain file to 4 KiB, pending commands to 128,
and command/partial-frame deadlines to 10 seconds. IDs and session IDs must
match outstanding commands. Events are handled while responses are pending.

SIGINT, SIGTERM and SIGHUP stop the loop. All exit paths send TERM then KILL to
the Chromium process group and reap its leader. Chromium also gets a parent-
death signal. An idle browser has no command deadline.

## Required integration boundary

The parent NixOS module must still provide and verify all of these:

- A trusted static portal service must own port 8099 throughout the kiosk's
  lifetime, including restarts. A port takeover serves code at the trusted
  origin. A supervisor-held listening socket is one way to avoid this gap.
- Game units must not see the brain directory, profile parent, control FDs, or
  browser/launcher process memory through `/proc` or ptrace. Mode 0700 alone
  does not isolate another process using the same UID. Use a separate service
  identity or verified namespace/process isolation.
- The service must use a cgroup with whole-group teardown. A process-group kill
  is not a replacement for the cgroup boundary if a descendant changes groups.
  The launcher removes stale ephemeral profiles after abnormal exits. The
  supervisor must retain the profile parent so game exclusions remain valid.
- Chromium's sandbox, Wayland/compositor setup and trusted executable/wrapper
  need device-specific verification. This launcher does not disable sandboxing.

All code served at the trusted origin is trusted with the capability. This is
not an XSS defense. Suppressed Chromium diagnostics make browser failures harder
to diagnose; the fixed launcher error class is intentionally less informative.
The launcher has no korrid crate dependency. Its NixOS module composes the
existing static server and host daemon. Compositor foreground control, gamepad
input delivery, and transparent overlays still require integration and device
verification.

## Verification

From the repository root:

```sh
services/kiosk/check.sh --chromium
```

This Nix-shebang gate runs rustfmt, nixfmt, Clippy with warnings denied, unit
tests and an opt-in real headless Chromium test. The test needs a sandbox-capable
non-root user and free loopback ports 8099 and 8100. Its static HTTP fixture
always returns 404 for runtime.json. It verifies private top-frame delivery,
atomic token refresh, exact-URL matching, child-frame refusal, refusal after
cross-origin top navigation, direct local HTTP refusal, nonlogging and SIGTERM
profile cleanup. No live backend or device is used.

The standalone Crane package uses the repository's pinned inputs without any
flake modification:

```sh
nix build --impure --no-link --print-out-paths --expr '
  let
    f = builtins.getFlake (toString ./.);
    pkgs = import f.inputs.nixpkgs {
      system = builtins.currentSystem;
      overlays = [ f.inputs.rust-overlay.overlays.default ];
    };
  in import ./services/kiosk/package.nix {
    inherit pkgs;
    crane = f.inputs.crane;
  }'
```

The package runs unit tests. The real Chromium test is opt-in and is not claimed
as a sandboxed Nix build test or an aarch64/device test.
