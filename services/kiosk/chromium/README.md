# Native app-window transparency experiment

Status: native transparency verified locally on x86_64; not deployed on Odin.
The user explicitly chose to pursue transparency after clarifying that opaque
overlays are also useful. Stock Chromium remains the production kiosk package
until launcher integration and ARM/device verification pass.

## Scope

The patch targets Chromium `143.0.7499.169`, from the repository's pinned
nixpkgs. It changes only Linux processes launched with both `--app=...` and
`--enable-transparent-visuals`. Invocations without both flags retain the
upstream behavior. This is a process-wide opt-in for a dedicated app process,
not a web permission or an instruction from arbitrary page JavaScript.

The candidate changes five upstream files under
`chrome/browser/ui/views/frame/`:

- `browser_widget.h` and `.cc` define the shared opt-in predicate and request
  an explicitly translucent native widget.
- `contents_web_view.cc` stops painting the native content background and
  marks its layer non-opaque.
- `multi_contents_background_view.cc` stops painting the additional background
  used by Chromium's multi-content view.
- `browser_desktop_window_tree_host_linux.cc` clears the native opaque-region
  hint. Otherwise Wayland treats transparent page pixels as opaque content.

The patch does not disable the sandbox, start another browser, expose a
DevTools TCP port, or change credential delivery. CSS still determines whether
the app draws an opaque hub or a transparent overlay. The native test now
passes with CSS alone, without a CDP background override.

## Verified patched result

The official x86_64 build completed in 43,118 seconds, about 12 hours:
`/nix/store/r0jmiqj5k4a3g9d4i63w7kdksr77whlc-chromium-143.0.7499.169`.

The native alpha test passed with this binary (`proc_5d09`). The no-flag opaque
control also passed (`proc_aafe`). These test results cover full-alpha,
half-alpha, and opaque pixels, both window sizes, and reuse of one native
nonfullscreen window for hub and overlay.

The optional Neverball underlay test also passed (`proc_4f8c`). This runs the
real game through Xwayland with software GL and a temporary HOME. It pauses
the disposable fixture for exact pixel comparisons, then verifies that its
animated menu resumes rendering behind the still-open overlay. It does not
exercise korrid's launch authority or the Odin GPU. Screenshot:
`/tmp/korri-native-alpha-eXXDBJ/game-resumed.png`.

Compositor screenshots are in `/tmp/korri-native-alpha-ifYUIh/`; the control
screenshots are in `/tmp/korri-native-alpha-3YSlV0/`. `blend-480.png` shows the
magenta underlay through transparent regions, a solid green control, and a
half-alpha green control blended to grey. This is not an Odin GPU or game test.

The CSS-only native test and opaque control also passed (`proc_6ab5` and
`proc_8c82`). The actual Rust launcher with the patched browser passed its
native bootstrap probe (`proc_66e9`): one app window, private runtime delivery,
and HTTP 404 for `/runtime.json`. Its compositor screenshot also shows native
transparency without changing the launcher's CDP commands.

An earlier candidate called `SetFillsBoundsOpaquely` on solid-color layers.
Source review caught that Chromium rejects this with a CHECK. Those calls
were removed before this successful build. Solid-color layers derive their
opacity from the color passed to `SetColor`.

## Evidence before the patch

`../tests/native-alpha.mjs` runs a native Wayland Chromium app above a magenta
Sway background. It captures the compositor with `grim`, not the renderer with
`Page.captureScreenshot`. The transparent test fails on stock Chromium: the
center pixel is white instead of magenta. Its opaque control passes, including
resize and switching between the hub and overlay in one window.

The test checks transparent, opaque, and half-alpha regions, two window sizes,
and repeated use of one nonfullscreen native window. By default it uses a
known compositor background; `KORRI_ALPHA_NEVERBALL` selects an absolute
Neverball executable for the real Xwayland underlay case. Real Odin GPU,
frame-time/memory measurements, launch authority, and input behavior remain
separate acceptance gates.

```sh
nix build .#korri-chromium --no-link --print-out-paths --cores 4 --max-jobs 1
services/kiosk/tests/native-alpha.mjs /absolute/patched-chromium/bin/chromium
services/kiosk/tests/native-alpha.mjs /absolute/patched-chromium/bin/chromium --opaque-control
KORRI_ALPHA_NEVERBALL=/absolute/neverball/bin/neverball services/kiosk/tests/native-alpha.mjs /absolute/patched-chromium/bin/chromium
```

The build retains nixpkgs' official release configuration, dependency set and
sandbox wrapper. It adds only the patch to the unwrapped browser derivation.
The version assertion requires a deliberate rebase and verification when
Chromium changes. This assertion is not permission to delay security updates.

## Cost and limits

Full Chromium compilation is required. The measured 12-hour build is for the
local x86_64 release configuration, not a prediction for ARM. The first experiment builds x86_64
locally. Fuji now has 27 GiB free and its read-only GC report shows zero
unreachable store paths (`proc_b2b0`); no GC was run for this check.

`korri-chromium-aarch64` selects the existing x86_64-to-aarch64 cross toolchain
used by the Odin kernel. Evaluation reports `aarch64-linux` as its target. The
ARM cross-build is in progress (`proc_49df`), not verified. Its initial build
plan requires 442 derivations and 2.3 GiB of cache downloads. This avoids
removing retained Fuji roots or compiling on the handheld. Do not clear
unrelated build roots or write internal Odin storage to obtain build capacity.

Per-pixel blending adds compositor work while the overlay is visible. Its
frame-time and battery impact are unmeasured. Visual transparency does not
imply pointer click-through and does not solve focus or controller routing.
