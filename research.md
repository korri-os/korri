# Research: Chromium 143 native Wayland per-pixel transparency

## Summary
Stock Chromium 143 failed the supplied native-Wayland transparency probes. I found no source-verified existing flag that fixes this, but I could not prove impossibility or identify an exact 143 patch: this worker has web search and local-file reads, but no URL-fetch or command-execution tool. Do not treat a guessed `kTranslucent` edit as an established one-line fix.

## Findings
1. **Verified locally by file inspection; execution results supplied by caller** — `/tmp/probe-chromium-native-alpha.mjs` launches Chromium **143.0.7499.169** with native Wayland and `--remote-debugging-pipe`. Sway disables Xwayland. The script compares CDP capture with `grim` after transparent CSS and `Emulation.setDefaultBackgroundColorOverride`. The caller reports alpha-zero CDP pixels but white compositor pixels, including with `--enable-transparent-visuals` plus ANGLE gles-egl and with `--disable-gpu`. This proves failure of those configurations, not every stock configuration. I did not rerun them. The existing probe tests magenta compositor wallpaper, not an external game.

2. **Upstream evidence supports a native-view obstruction, not a CSS problem** — Chromium's historical commit `e21c18aae29ee25c524b75f1c499aa2ac159a6e5` explicitly describes transparent system web-app work: “1. make BrowserView widget translucent 2. disable window backdrop (normally visible in tablet mode) 3. set transparent background color on ContentsWebView”. This is an indexed upstream commit-message excerpt, not a fetched patch. It establishes multiple native layers as relevant. It does not establish the exact Linux 143 obstruction or a supported command-line switch. [Chromium commit](https://chromium.googlesource.com/chromium/src/+/e21c18aae29ee25c524b75f1c499aa2ac159a6e5)

3. **Do not claim Wayland itself cannot carry Chromium alpha** — Chromium commit `ff7fb86b9250accade5f4ad58a3b8f2d45c0d568` says: “This CL implements WaylandWindow::IsTranslucentWindowOpacitySupported(), which enables rounded corners on Wayland when CSD are used.” This is another indexed upstream commit-message excerpt. Transparent decoration corners do not prove transparent web-content pixels, but they refute a blanket assertion that Chromium's Wayland windows cannot support translucency. [Chromium commit](https://chromium.googlesource.com/chromium/src/+/ff7fb86b9250accade5f4ad58a3b8f2d45c0d568)

4. **Opaque regions and pixel format must be checked separately from renderer alpha** — The Wayland specification says: “A NULL wl_region causes the pending opaque region to be set to empty.” An empty opaque region is necessary when no pixels can be promised opaque; it does not create alpha in an opaque buffer. The next probe should record both region requests and actual buffer format. Clearing an opaque region alone is not a demonstrated Chromium fix. [Wayland protocol specification](https://wayland.freedesktop.org/docs/html/apa.html)

5. **Embedding changes the native-window contract, but needs a decision** — Electron documents a `transparent` native-window constructor option. Its options documentation says: “Alpha in #AARRGGBB format is supported if transparent is set to true. Default is #FFF (white).” This is a concrete embedding API, not a stock Chromium flag or a verified Sway result. [Electron window styles](https://www.electronjs.org/docs/latest/tutorial/custom-window-styles) · [Constructor options](https://www.electronjs.org/docs/latest/api/structures/base-window-options)

| Option | Evidence and feasibility | Cost / unresolved risk |
|---|---|---|
| Keep stock Chromium | Current tested configurations fail; no verified enabling flag found | No honest path to declare transparency supported yet |
| Patch Chromium 143 | Upstream native transparency work identifies widget and ContentsWebView layers worth tracing | Full Chromium build and ongoing patch maintenance; patch size and sufficiency are unknown; per-window scoping must leave hub opaque |
| Electron | Documented transparent BrowserWindow API; multiple windows can share one application/browser instance | Adds embedding/application ownership outside the approved Rust-launcher shape; native Wayland/Sway behavior needs a real probe |
| CEF | Historical CEF transparency discussions explicitly modify widget opacity | Native Wayland windowed transparency was not verified; offscreen embedding would add render, input, and window integration, not a tiny launcher change |

One browser instance must own the hub and overlays in every candidate. Chromium renderer/GPU subprocesses are distinct from launching a separate browser instance per overlay. Preserve the Rust launcher's private CDP pipe, enabled sandbox, and non-HTTP capability handling. No embedding decision is made here.

## Exact source inspection still required
These URLs are pinned to the version found in the probe. **They are inspection targets, not fetched or verified source citations.** No unavailable full-source contents are quoted as fact.

- [browser_frame.cc](https://chromium.googlesource.com/chromium/src/+/refs/tags/143.0.7499.169/chrome/browser/ui/views/frame/browser_frame.cc): browser widget initialization and opacity selection.
- [browser_desktop_window_tree_host_linux.cc](https://chromium.googlesource.com/chromium/src/+/refs/tags/143.0.7499.169/chrome/browser/ui/views/frame/browser_desktop_window_tree_host_linux.cc): browser-specific decoration/content hints.
- [desktop_window_tree_host_platform.cc](https://chromium.googlesource.com/chromium/src/+/refs/tags/143.0.7499.169/ui/views/widget/desktop_aura/desktop_window_tree_host_platform.cc): native widget-to-platform opacity mapping.
- [wayland_window.cc](https://chromium.googlesource.com/chromium/src/+/refs/tags/143.0.7499.169/ui/ozone/platform/wayland/host/wayland_window.cc): window opacity, bounds, and region propagation.
- [wayland_surface.cc](https://chromium.googlesource.com/chromium/src/+/refs/tags/143.0.7499.169/ui/ozone/platform/wayland/host/wayland_surface.cc): actual `wl_surface` opaque-region requests.
- [gl_surface_wayland.cc](https://chromium.googlesource.com/chromium/src/+/refs/tags/143.0.7499.169/ui/ozone/platform/wayland/gpu/gl_surface_wayland.cc): candidate GPU surface path; trace the actually selected buffer implementation rather than assuming this file owns it.

## Feasible next probe
1. Repeat the existing script with `WAYLAND_DEBUG=client` in Chromium's environment. Its stderr already goes to `chromium.log`. Capture `wl_surface.set_opaque_region`, the associated region rectangles, commits, and buffer creation/import messages. Match the top-level content surface and any subsurfaces; do not confuse a decoration surface with content. Keep all existing sandbox and private-pipe settings.
2. Use the software run to make `wl_shm` formats easier to inspect. With GPU rendering, inspect advertised/imported dmabuf formats instead. Determine whether the content buffer carries alpha and whether Chromium declares the content region opaque. This separates two candidate obstructions but does not inspect final buffer pixel values.
3. After fetching the pinned source, change only the demonstrated obstruction in an experimental build. Recheck the existing green-patch/magenta-background test. A native-view background can still fill white even after requesting an alpha-capable widget; test before calling the patch sufficient.
4. Only after that passes, place the window above a moving external game under Sway. Use one browser instance for an opaque hub and a transparent overlay. Validate an alpha-zero region, an alpha-one control, and a half-alpha control against known underlay colors. Separately verify Sway stacking and input behavior; transparency does not imply click-through or positioning authority.

## Sources
- Kept: Chromium transparent SWA commit — primary upstream evidence that native widget and content backgrounds require explicit handling; historical, not 143 proof.
- Kept: Chromium Wayland translucent-opacity commit — primary upstream evidence against blanket backend incapability; historical, not 143 proof.
- Kept: Wayland protocol specification — authoritative opaque-region semantics.
- Kept: Electron official window documentation — concrete alternative embedding API; not a tested Sway result.
- Dropped: Reddit/Ubuntu reports of accidentally transparent Chromium windows — unrelated rendering failures, not supported per-pixel app-window transparency.
- Dropped: Historical Stack Overflow `INFER_OPACITY` to `TRANSLUCENT_WINDOW` patch — stale global default mutation, not a scoped 143 fix.
- Dropped: Old CEF transparent-overlay forum recipe — useful search lead, not sufficient native Wayland evidence.

## Gaps
The requested exact Chromium 143 source obstruction, minimal existing flag/feature, and tested minimal patch remain **unresolved**. Full-source fetch and a real execution environment are required. Search API accepts only singular `query`, not the requested `queries`/`workflow`; three research angles were searched separately. Parallel initial searches hit provider rate limits, so follow-ups ran sequentially. A Jina attempt failed because no API key is configured. No repository runtime source was edited.
