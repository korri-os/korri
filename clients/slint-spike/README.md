# THROWAWAY SPIKE: native Slint window over the real korrid

This is a prototype on a throwaway branch. It is not a product client. It is
not a surface. It does not define a native surface treaty.

## The question

Can a native Rust + Slint window, with no browser and no WebView, start the
real current korrid and show its state from the real typed RPC? Can its
Refresh control make a real authenticated loopback call? Can it do this
without network effects outside the machine?

## The answer

Yes, on an x86_64 Linux workstation with X11 (Xvfb) and Slint's software
renderer. The window starts korrid through its public embedded entry point.
It reads `system.health`, `system.identity.status`, `app.catalog.snapshot`
and `app.peer.list` through `korrid::RpcRequest` / `korrid::RpcResponse`.
Refresh repeats those four calls with the per-run bearer capability.

This does **not** prove anything about the LG B7, webOS, its ABI, graphics,
remote input, startup, Home behaviour or a launcher. It is a desktop proof
only.

## Run

From the checkout:

```sh
nix run .#slint-spike          # the window (needs an X11 DISPLAY)
nix run .#slint-spike-check    # fmt, clippy, unit tests, Xvfb self-test, network audit
nix run .#slint-spike-proof -- OUTPUT_DIR   # screenshots of the real X11 window under Xvfb
nix run .#slint-spike-build    # build only
```

The first build compiles korrid and Slint from source (about 25 minutes with
2 jobs on the spike's build host). Builds use 2 jobs unless `CARGO_BUILD_JOBS`
is set. The spike uses its own target directory, `.cache/slint-spike-target`.
It never writes `.cache/korrid-target`.

Keys: Tab and Shift+Tab move focus between Refresh and Close. Return or Space
activates the focused control. Refresh has focus at start. The first check
runs at start through the same callback as Refresh.

## What it shows

| Section | Source |
| --- | --- |
| Status line | `system.health` decides healthy or not. HTTP 401/403 shows "korrid refused this window". A closed port shows "korrid is not reachable". Neither ever shows as healthy. A typed `Err` on another call shows "answered with problems". |
| Connection | Loopback only; credential held in memory; check number, answered count, round trip. No port and no capability is shown or logged. |
| Device identity | `IdentityState` from `system.identity.status`. A fresh scratch device is `Unowned`. |
| Catalog | `CatalogSnapshot.games` and `failures`. The scratch device has no games: "No games in this device's catalog." This is the isolated scratch brain, not the user's library. |
| Peers | `PeerList.peers`. The scratch device has none. |
| Build | Spike version with `KORRI_BUILD_ID` (the CLI's existing stamp name, `clients/cli/package.nix`), and the version string from `system.health` (`korrid-v0`). |

## Isolation and secrets

| Concern | What the spike does | Grounding |
| --- | --- | --- |
| Storage | Two new temporary directories. The private root gets mode 0700. Both are removed on Close, window close, SIGINT or SIGTERM, after korrid stops. | `federation/store.rs` `private_directory` requires exactly 0700; `embedded_tests.rs` sets it the same way. |
| WAN relays | `EmbeddedPlatform::default()` gives `relays: None`. A fresh storage root has no `host.relays`. Discovery returns before any relay work. | `lib.rs` `start_local_server_for_platform`; `relay.rs` `RelayList::from_device_settings`; `federation/coordinator.rs` `cycle` returns when `relays` is `None`. |
| LAN / peer reach | `reach: None`, so no iroh endpoint starts. | `lib.rs` `start_local_server_for_platform`, `reach_environment.and_then(...)`. |
| Inherited peer config | The binary refuses to start if any `KORRID_*` or `KORRI_*` name other than `KORRI_ROOT` and `KORRI_BUILD_ID` is set. It prints names only. | `upstreams.rs` `from_env_or_file` reads `KORRID_UPSTREAMS` regardless of the storage root. |
| Capability | Read once from `korrid::local_server_capability()` into a `Zeroizing<String>`. Not in args, env, UI, logs, files or screenshots. `Brain` has no `Debug`. Core dumps are off. | `clients/linux/README.md` keeps both connection values out of logs and assets. |
| Proxy | The HTTP client uses `no_proxy()`, so an environment proxy never receives the bearer. | reqwest system-proxy default. |
| Browser origin | `https://portal.example`, the value korrid's own embedded tests use. No browser takes part, and the `.example` TLD cannot serve a page. | `embedded_tests.rs`. |
| Time | One 4 s budget for all four calls of a refresh. Shutdown waits for a refresh in flight before it stops korrid. | `brain.rs` `INSPECTION_BUDGET`. |

The check app runs the self-test under `strace -e trace=connect -s 0`. The
trace goes straight into `awk`. Only counts per socket family are kept: no
port, path or raw trace line. `connect(2)` carries an address, never a
request body, so the bearer cannot enter the trace.

## Evidence

Verified on the spike's build host (x86_64 NixOS, Xvfb, no window manager):

- `nix run .#slint-spike-check` passed. It ran fmt, clippy `-D warnings`, 8
  unit tests (state mapping, layout plan boundaries), the
  `KORRID_UPSTREAMS` refusal, and the self-test.
- The self-test opens the real window. It invokes the Refresh callback twice.
  After each refresh it reads the window's `report` back and checks: healthy,
  `korrid-v0` shown, scratch device `Unowned`, catalog empty, Refresh focused.
  It also sends `system.health` with no bearer (401), a wrong bearer (401) and
  the real bearer (200).
- Network audit: `AF_INET loopback 1`, `AF_UNIX 3`. No IP `connect(2)` went
  anywhere other than 127.0.0.1. The AF_UNIX connections are the X server and
  similar local sockets. Scratch directories were gone afterwards.
- `nix run .#slint-spike-proof` drove the real window with XTest input after
  `xdotool windowfocus`: Tab moved focus to Close, Shift+Tab and Return ran
  check 2, a mouse click on Refresh ran check 3, and Tab + Return on Close
  exited and removed the scratch storage.

Screenshots are X11 captures (`import -window`) of the real winit window,
rendered by Slint's software renderer on Xvfb. They are not an offscreen
render and not an HTML stand-in. There is no compositor. They were made by
`nix run .#slint-spike-proof` from commit `14fa80689`, so the window shows
`korri-slint-spike 0.0.0 (14fa8068907a)`. That stamp names the source commit
of the build, not the later commit that added these images.

Seven are committed in `evidence/`:

| File | Shape | State |
| --- | --- | --- |
| `01-generous-1440x900-refresh-focused.png` | generous | healthy, Refresh focused |
| `02-generous-after-tab-key.png` | generous | after Tab: Close focused |
| `03-generous-return-key-refresh-check-2.png` | generous | Shift+Tab, Return: check 2 |
| `06-narrow-tall-390x844.png` | narrow-tall | one column, actions under the title |
| `07-wide-short-1280x300.png` | wide-short | three columns, compact, actions inline |
| `09-unauthorized-1100x700.png` | medium | real korrid, wrong bearer: refused |
| `12-offline-wide-short-1280x300.png` | wide-short | real korrid stopped: not reachable |

All twelve from that run, including check 3 (mouse click), 960x640, 320x240
and the other error shapes, are kept outside the repository at
`~/.cache/korri-slint-spike-evidence/14fa80689/` on the build host.

## Limitations

- x86_64, X11 and the software renderer only. No Wayland, no GPU renderer,
  no accessibility bus (`accessibility` feature off), no system tray.
- No real games, peers or owner: the scratch device is honestly empty and
  unowned. The "many games" and long-title cases of the stress set are not
  reachable without inventing data, so they are not shown.
- The content region scrolls with a pointer. Its readouts are not focusable,
  so a keyboard or remote cannot scroll it yet. Refresh and Close stay on the
  surface at every size tried (down to 320x240).
- Layout thresholds (340 px column, 560 px inline header, 480 px compact) are
  guesses checked by eye, not measured on a TV.
- `--proof-failure=unauthorized|offline` exists only to show real failure
  states: a fresh random bearer to the real server, or the real server
  stopped before the first refresh.
- The Nix apps run cargo in the checkout like `nix/tasks.nix`; they are not
  a hermetic package.

## Treaty position

`contracts/surface/korri-surface.ts` mounts a surface into an `HTMLElement`
and drives DOM focus. This window does not implement or extend that treaty.
It is a standalone host spike in `clients/`, not in `surfaces/`. A
production native presentation needs its own contract decision first.
