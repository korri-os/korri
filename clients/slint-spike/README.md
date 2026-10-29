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
`~/.cache/korri-slint-spike-evidence/14fa80689/` on the build host. That
folder is kept for the life of this experiment. An annotated after-only crop
of image 03 is at
`~/.cache/research/korri-lg-b7-prep-20261029T175543Z/slint-native-proof.png`.

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

## webOS SDL adapter (THROWAWAY experiment)

Question: can the same view and the same real korrid run through the SDL2
that the LG B7 already ships, instead of winit/X11? This section covers the
adapter and its workstation proof only. Nothing here was run on the TV.

### Build features

| Feature | Window system | Build |
| --- | --- | --- |
| `desktop-x11` (default) | Slint winit X11 backend, as before | `cargo build` |
| `webos` | Slint custom platform over the system `libSDL2-2.0.so.0`, loaded with `dlopen` (`src/sdl/`) | `cargo build --no-default-features --features webos` |

The two features exclude each other (`compile_error!`). The `webos` build
links no SDL, X11, Wayland, freetype or fontconfig library; on x86_64 its
`NEEDED` list is libc, libm, libgcc_s and the loader. fontconfig is opened
at run time when present (`fontique/fontconfig-dlopen`); the view embeds
Slint's own Inter font (`ui/fonts/`, OFL), so text does not need a TV font.

### What the adapter does

- Binds 27 SDL functions, all in SDL 2.0.4's export list
  (`src/dynapi/SDL_dynapi_procs.h` of the 2.0.4 release tarball). Struct
  offsets and constants come from the 2.0.4 headers.
- Sets `SDL_VIDEO_ALLOW_SCREENSAVER=1` before `SDL_Init`; calls
  `SDL_EnableScreenSaver` after it; refuses to open the window if
  `SDL_IsScreenSaverEnabled` is still false. SDL 2.0.4 otherwise disables the
  screen saver in `SDL_VideoInit` (`src/video/SDL_video.c`). The loop draws
  only when Slint asks; an idle window renders no frames.
- Sets no webOS hints. Home, Exit and the coloured keys are not grabbed.
- Software renderer into one buffer; dirty rectangle uploaded into one
  `ABGR8888` streaming texture; `SDL_RenderCopy` and `SDL_RenderPresent`.
  Accelerated renderer first, any renderer if that fails.
- Scale: output height / 720 (1080 lines give 1.5, a 1280x720 logical view).
  This is a guess for viewing distance. `SLINT_SCALE_FACTOR` overrides it.
- Input at the host edge (`src/sdl/keys.rs`): Up/Left send Backtab, Down/Right
  send Tab (the view is one focus chain: Refresh, Close); Return and keypad
  Enter confirm; Escape or `SDL_SCANCODE_AC_BACK` close on key release.
  Pointer and wheel events pass through. Every key press logs its scancode
  and keycode (numbers only), so a TV run shows what the remote sends.
- Lifecycle: `SDL_QUIT`, `SDL_APP_TERMINATING`, window close, SIGINT and
  SIGTERM all take the normal shutdown, which stops korrid and removes the
  scratch storage. Hidden or minimised windows stop drawing.
- With no X11 `DISPLAY`, sets `EGL_PLATFORM=wayland` and
  `XDG_RUNTIME_DIR=/tmp/xdg` when unset, as Moonlight TV's `main()` does on
  webOS (`src/main.c` at `89221f66`).

### Arguments (webos build)

| Argument | Meaning |
| --- | --- |
| none | Fullscreen window over the desktop mode of display 0. |
| one JSON object | webOS launch parameter (Moonlight TV `app_handle_launch` reads `argv[1]` as JSON). At most 16 KiB. Counted, dropped, never printed. Anything else that starts with `{` is refused. |
| `--windowed=WxH` | Resizable window, for the workstation check. |
| `--self-test`, `--proof-failure=unauthorized\|offline` | As in the desktop build. |

The webos build exits with status 2 before korrid or SDL starts when the
effective user is root, or when any `KORRID_*`/`KORRI_*` name other than
`KORRI_ROOT` and `KORRI_BUILD_ID` is set. The port and capability stay in
memory, as before.

### Run and check (workstation)

```sh
nix run .#slint-spike-sdl -- --windowed=1920x1080   # upstream SDL 2.0.4, X11
nix run .#slint-spike-sdl-build                      # build + test binaries
nix run .#slint-spike-sdl-check                      # see below
nix run .#slint-spike-sdl-proof -- OUTPUT_DIR        # screenshots, keys, click
```

`sdl204.nix` builds upstream SDL 2.0.4 (X11 only) as a test fixture. It is
never shipped. LG's 2.0.4 carries webOS patches this build lacks.

### Evidence (commit `34f3eb0c4`, x86_64 NixOS, Xvfb, no window manager)

`nix run .#slint-spike-sdl-check` passed: fmt; clippy `-D warnings` and 12
unit tests with `--features webos`; 27 of 27 bound functions in 2.0.4's
export list; no window-system library in `NEEDED`; euid 0 refused (user
namespace with uid 0 mapped); bad launch JSON and bad flags refused without
echo; the self-test (real korrid, two Refresh callbacks, 401/401/200 auth
probes) passed over upstream SDL 2.0.4 and over nixpkgs sdl2-compat 2.32.72,
each reporting `screen saver allowed: true`; network audit
`AF_INET loopback 1`, no IP outside loopback; a launch JSON object opened
the fullscreen window, ran the first refresh and shut down on SIGTERM with
the scratch storage removed.

`nix run .#slint-spike-sdl-proof` over upstream SDL 2.0.4 (renderer
`opengl`): Down moved focus to Close, Up and Return ran check 2, a mouse
click on Refresh ran check 3, Escape closed the window and removed the
scratch storage. Debug-build frame times on the workstation: first frame
166 ms render + 103 ms upload/present at 1920x1080; 7 frames in the whole
run. These are not TV numbers.

Committed: `evidence/sdl-01-1920x1080-refresh-focused.png`,
`sdl-02-down-focuses-close.png`, `sdl-04-mouse-click-check-3.png`. All five
images and both logs are at
`~/.cache/korri-slint-spike-evidence/34f3eb0c4-sdl/` on the build host.

### Not known until the TV run

- Whether LG's SDL 2.0.4 opens a Wayland window for a process that the
  application manager starts, and whether it needs an app ID. Moonlight TV
  does not use the system SDL: it ships SDL 2.30.12 (webOS backport) in
  `lib/backports/` with an `RPATH`, and that build reads `APPID`.
- Which key code the remote's Back key produces, and whether the compositor
  delivers it to the app at all without a webOS key policy hint.
- Whether the application manager expects the app to register over Luna
  (the backport has `registerNativeApp` support; this adapter makes no Luna
  calls).
- Release-build frame time on the TV's ARMv7 CPU, and memory use: one
  1920x1080 buffer and one texture are about 8 MiB each.

The desktop build still passes `nix run .#slint-spike-check` at `34f3eb0c4`
(fmt, clippy, 8 unit tests, X11 self-test, network audit `AF_INET loopback 1`).
It now renders with the embedded Inter font as well.
