# Web session progress

The web session is incomplete and is not deployed. Completed foundations are
committed in `.worktrees/feat/odin2portal-web-session` on
`feat/odin2portal-web-session`, rebased onto `main` at `43e1d6aa`.
Do not activate this configuration until the remaining gates pass.

## Current integration checkpoint

The six foundation commits replayed onto current main. Two follow-up commits
preserve exact stop identity and verify browser freeze/thaw after the merge:
`fabc3c08` and `126ff44c`. Main's signed peer sessions, statistics, recovery
rollback, and freeze/thaw remain in the branch. No second lifecycle authority
was introduced.

Post-rebase checks passed: 483 korrid tests, 264 Portal tests and TypeScript,
25 kiosk unit tests, and the korrid-device, Linux-host, and kiosk module checks.
The explicit Chromium integration was subsequently rerun with the patched
x86_64 Chromium 143 wrapper: 25 kiosk unit tests and the private-pipe integration
passed, along with formatting and Clippy (`proc_1c80`). This does not establish
ARM or Odin acceptance.
Logs are `/tmp/rebase-korrid-final.log`, `/tmp/rebase-portal-final.log`, and
`/tmp/rebase-module-check.log`. The system closure below predates the rebase
and must be rebuilt before deployment.

Read-only device record `proc_1597` confirms Linux 7.2.0 with root and boot on
`/dev/mmcblk0p2` and `/dev/mmcblk0p1`. The four web/input services are absent.
The controller reports `AYN Odin2 Gamepad`, bus `0003`, vendor `2020`, product
`3001`, currently `event3`. Touch reports `generic ft5x06 (44)`, currently
`event4`. Event numbers are observations, not stable device identities.
The SD root had 431 GiB free at that device check. Earlier Fuji capacity
readings are superseded by the native-build checkpoint below.

Chromium 143 source inspection confirms two native opacity layers:
`chrome/browser/ui/views/frame/contents_web_view.cc:121-143` fills the native
content layer with the theme background when `background_visible_` is true;
`browser_desktop_window_tree_host_linux.cc:223-281` declares the content region
opaque to Wayland. These sources were downloaded from the exact upstream tag
`143.0.7499.169` into `/tmp/chromium-143-alpha-source/`. The user approved the
Chromium transparency patch. It now passes local native compositor tests and
is committed as `bbc6a5d8`; the Rust/private-pipe design remains unchanged.
ARM compilation and Odin GPU verification remain pending.

## Verified checks

| Check | Result |
|---|---|
| `nix run .#portal-check` | 255 tests passed, zero failed; TypeScript passed. Process `proc_afb5`. |
| `nix run .#korrid-test` | Full suite passed after mandatory browser/profile exclusions. Process `proc_d89b`. |
| Host and kiosk NixOS module checks | Both passed after persistent directory provisioning and `drm,libinput`. Process `proc_ee51`. |
| Kiosk checks with pinned Chromium 143 | 25 unit tests passed, plus the explicit Chromium integration test. Formatting and Clippy passed. Process `proc_da00`. |
| Static Portal package | Built `/nix/store/gqnarnwcg7br9rnmbfn7fffngbvjz6mc-korri-portal-0.0.0`. Install checks compare the blank bootstrap asset and reject a public `runtime.json`. |
| Native Wayland launcher probe | One output-sized floating app window, no tabs/address bar, no fullscreen, private runtime delivery, public HTTP runtime request returns 404. Process `proc_210c`. |
| Full Odin system build | Latest bootstrap/rule build passed: `/nix/store/i7d23qlas7hr932xyr6q2aj0r3bgf7mx-nixos-system-odin2portal-sd-card-26.05.20251221.a653104`. Process `proc_d095`, exit 0. Rescue closure: `/nix/store/88q0x9mwvz6sn4pfcqfjbk52s02k6ny0-nixos-system-odin2portal-linux-7.0.2-rescue-sd-card-26.05.20251221.a653104`. |
| Device state | Last read-only SSH returned Linux `7.2.0`; `/var/lib/korri` was absent. No web-session activation or device writes occurred. |

The native Wayland probe ran locally on headless Sway, not on the Odin GPU.
It used the real Rust launcher, Chromium 143, production browser flags, and
the actual Odin app-window rule extracted from `web-session.nix`. Its runtime
file contained only a test capability. The test HTTP server never served it.

Probe script: `/tmp/probe-korri-native-window.mjs`.
Screenshot: `/tmp/korri-native-window-4JyLjL/native-window.png`.

## Implemented

- The user approved the Rust launcher and private Chromium pipe. The launcher
  intercepts only the exact runtime request from the trusted top frame. It
  rejects child frames and document requests. Navigation away revokes access.
  The static server never provides a capability endpoint.
- The browser listener shares `HostRuntime` through `RpcSurface::LocalBrowser`.
  Catalog, prepare, status, and exact-identity stop use real host behavior.
  Signed peer RPC and inputd's private control authority remain separate.
- Production Linux no longer creates a fixture Android bridge. Unsupported
  native facts and settings are omitted. Stop sends the displayed launch ID.
  Tests cover duplicate actions, stale responses, unmounts, and explicit null
  idle status from Rust.
- The static package uses the existing Bun locks. Its dependency hash remains
  verified for x86_64-linux only; its output is architecture-independent data.
  No npm lockfile or Bun production server remains.
- The NixOS modules compose the shared Sway host, inputd, korrid, Rust static
  server, Chromium launcher, and software Sunshine. Runtime surface selection
  remains outside the static build. The Odin keeps UID/GID 1000, landscape
  transform 270, touch output mapping, and the Linux 7.0.2 rescue entry.

## Corrections from actual tests and review

- Optional missing-path exclusions were unsafe. A game started before a
  directory existed could see that directory after kiosk startup. Tmpfiles
  now provisions both private parents before launches. Game exclusions are
  mandatory. The kiosk retains its parent across stops and cleans only stale
  Chromium profile children under the existing lock. Old game namespaces
  must stop before a deployment introduces these protections.
- Happy DOM replaced Bun's HTTP objects and caused invalid test-server
  responses and preflight requests. The preload now retains the native HTTP
  and abort stack, including its matching `DOMException`. The original
  timeout assertions remain intact and pass.
- Physical readiness now reads `current_mode` dimensions, not rotated logical
  dimensions. The physical Sway backend includes libinput, not DRM alone.
- `--app=about:blank` opens a tabbed native Chromium 143 window. Headless
  success did not prove native app behavior. Native startup now uses an empty,
  CSP-protected `kiosk-blank.html` on the fixed Portal origin. The launcher
  installs interception before navigating to the real Portal. Headless tests
  still start at `about:blank`.
- Native target metadata can announce the bootstrap URL before the document
  commits. The captured frame initially had URL `:` and origin `://`. Startup
  now waits within a fixed deadline for the exact committed bootstrap frame.
  It grants no credential access during that wait.
- Native app mode ignores `--class` for its Wayland identity. The observed
  `app_id` is `chrome-127.0.0.1__kiosk-blank.html-Default`. The Odin rule matches
  that identity, removes borders, and makes the window output-sized and
  floating. The native probe verifies the actual rule.

## Transparent-window result

Native transparency is now verified locally on x86_64. The five-file opt-in
patch (`bbc6a5d8`) built successfully in 43,118 seconds and produced:
`/nix/store/r0jmiqj5k4a3g9d4i63w7kdksr77whlc-chromium-143.0.7499.169`.
A temporary GC root at `/tmp/korri-chromium-alpha-verified` retains it.

`services/kiosk/tests/native-alpha.mjs` passes with CSS alone: transparent,
opaque, and half-alpha pixels, two sizes, and hub/overlay reuse in one native
window. The no-flag opaque control passes. The actual Rust launcher also
passes private bootstrap with this browser and flag. No new CDP commands are
needed for native alpha (`f86e1b4d`).

The real Xwayland Neverball underlay test passes (`5e90cffc`). It pauses the
fixture for exact pixel comparisons, then proves the animated menu renders
again behind the persistent overlay. This uses software GL, not the Odin GPU,
and does not exercise korrid launch authority. Screenshot:
`/tmp/korri-native-alpha-eXXDBJ/game-resumed.png`.

The Zao cross-build (`proc_49df`) failed after 11,914 seconds. Its host-side
fontconfig binding generator targeted x86_64 but consumed AArch64 glibc headers,
which produced unknown SVE type errors. The cross-build remains stopped.

A matched Clang 21.1.2 benchmark used four real Chromium C++ files with an ARM
target. Output hashes matched on both machines. Zao achieved 1.78 times Fuji's
throughput at four workers, and 2.27 times with eight workers against Fuji's
four. Preprocessing and final linking were excluded. This does not establish a
whole-build ETA, especially because cross-compilation adds host-toolchain work.
Evidence: `/tmp/korri-chromium-speed-kbyhx998/summary.json` on Zao.

### Native build checkpoint, 2026-09-07

The native `packages.aarch64-linux.korri-chromium` build is running on Fuji.
The first native run stopped after 5,981 of 55,237 Ninja steps because its
observer treated `ProcessLookupError` from an exiting compiler as fatal. The
observer stopped the build, not Chromium or the resource limits. Its failed
source tree was retained. A clean retry repeats that work.

The verified package
derivation is `/nix/store/hf73x4j6345i9d3jvyq1plia4g3bhwc1-chromium-143.0.7499.169.drv`.
Only the already-reviewed transparency patch is applied. Release flags and the
sandbox remain enabled.

The current build unit is `korri-chromium-arm-build-56k4824i.service`, not a
client of the shared Nix daemon. It runs the local Nix store with one build and
four compiler jobs. Kernel cgroup checks verified a 16 GiB memory maximum,
14 GiB memory-high threshold, and zero swap allowance. The durable supervisor
stops only this build if free disk falls below 12 GiB. It survives SSH loss.
The shared Nix daemon remains PID 1891; no host activation was performed.

The ESRCH fix passes six supervisor tests. Review then found two more observer
risks: successful transient-unit results can disappear before they are read, and
failed diagnostic writes can bypass cleanup. A separate corrected guard took over
without restarting the build. Nix PID 481716 remained unchanged. Six additional
guard tests pass, including cgroup removal at completion and failed receipt writes.
A follow-up review found no remaining defects within that operational scope.

The guard proves package realization through registration of the exact Nix output
paths. It does not infer success from a vanished systemd result. It stops the owned
build before attempting failure diagnostics. The earlier observer is retired.

At the retry checkpoint the compiler had completed 2,872 of 55,237 Ninja steps.
Free space was 60.8 GiB, peak cgroup memory was 7.3 GiB, and no OOM events had
occurred. These observations do not predict final-link memory or build duration.

Current records on Fuji:
- `/var/lib/korri-chromium-build/run-56k4824i/build.log`
- `/var/lib/korri-chromium-build/run-56k4824i/guard.log`
- `/var/lib/korri-chromium-build/run-56k4824i/completion.json`

Guard unit: `korri-chromium-arm-guard-56k4824i.service`. Managed process
`proc_277c` waits for it, then copies successful outputs to Zao, adds GC roots,
and verifies stored contents. `proc_aa76` follows compiler logs with failure and
final-link alerts. Zao evidence and the tested operator scripts live under
`/home/simonwjackson/artifacts/korri-chromium-native/run-56k4824i/`.
No completed ARM artifact or Odin acceptance is claimed yet.

The input derivation has a dedicated GC root. Successful outputs will be rooted
under the run directory. Two earlier startup attempts stopped before compilation:
the first exposed a missing launcher PATH; the second found the previously
unpinned derivation absent. Both were corrected before the first native run.

The following records describe the original stock-browser failure. The local Chromium 143 probe
found an opaque native window even when CDP captured a page pixel with alpha
zero. The compositor screenshot showed white at the same location instead of
its magenta background. Both GLES/ANGLE and GPU-disabled trials with
`--enable-transparent-visuals` remained opaque. This is evidence about the
specific tested configuration, not proof that every Chromium integration
must be opaque.

Probe: `/tmp/probe-chromium-native-alpha.mjs`.
GLES evidence: `/tmp/korri-alpha-BJSAfB/`.
Software evidence: `/tmp/korri-alpha-D5JFeC/`.
Do not treat transparent CSS or `Page.captureScreenshot` as proof of native
window transparency.

## Remaining deployment gates

Rust already reconciles natural unit completion when session status is requested.
The Linux Portal now observes that status once per second while Ready (`2c62a3e4`).
The policy comes from the actual legacy consumer,
`product/platform/react/library/library-atoms.ts`, using `Atom.withRefresh` at one
second through `ForegroundSessionStatusLayerLive`. No legacy schema was imported.
The current Portal continues to use its existing local authenticated session RPC.

Session-only reconciliation preserves catalog, device facts, notices, and action
locks. It rejects stale responses after reload, prepare, stop, and unmount; it
skips overlapping observations. A failed query keeps the last known session and
does not prove exit. Unchanged observations preserve state identity.

`nix run .#portal-check` passed 274 tests and TypeScript (`proc_b5df`). Coverage
includes actual HTTP null-idle replies and controlled timer/race tests. Two
independent reviewers found no Portal defects. These tests do not constitute a
real game-exit or controller test on Odin. Exact-session freeze/thaw exists, but
compositor resume does not follow from process thaw alone.

1. Connect launch, return, and resume to compositor focus and browser
   presentation. The floating opaque hub currently covers normal game
   windows. Linux resume explicitly reports unavailable.
2. Deliver real Odin controller actions to the Portal without exposing raw
   hardware to JavaScript. Verify directions, confirm, back, overlay opening,
   touch, and return to the game. Ground InputPlumber mapping in device records.
3. Verify the new Linux session observer after a real game exits on Odin.
   Portal tests pass, but the hardware behavior remains unverified.
4. Complete the ARM Chromium build and verify native transparency on the Odin,
   then wire partial/full overlay presentation without adding another browser
   instance. Local native transparency is already proven; do not repeat the
   engine-selection discussion or restore the earlier claim that it is unresolved.
5. Establish a real device library and verify launch and exact stop. An empty
   catalog is not end-to-end evidence. Host settings/discovery remain
   unsupported; do not present them as working.
6. Verify same-UID game isolation, socket ownership, runtime-file readiness,
   cold boot, service restart recovery, and cgroup cleanup on NixOS. Review
   restart propagation explicitly: a dependency stopping the kiosk does not
   by itself prove automatic kiosk startup when that dependency returns.
7. Review and commit the finished slices, then deploy through the SD-safe
   workflow, reboot, and measure memory on the Odin and RG353M. Preserve the
   rescue entry. Never write firmware or internal storage for these checks.

`context.md` is scratch reconnaissance and must not be committed accidentally.
