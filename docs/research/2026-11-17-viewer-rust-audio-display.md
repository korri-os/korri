# Research: audio and display code in Rust for the PC viewer (step F of korri-os/korri #391)

Date: 2026-11-17. Labels: **verified** (I read the source named), **inferred** (consistent with the sources, not proven), **opinion** (my judgement).

Scope: the PC (Linux x86_64) viewer's audio and display path, at `02cb62c02`. Step F is audio and display, not video decode; video decode is listed only where the display code is tied to it.

## 1. What is not Rust in that path today

All verified, in `clients/viewer` unless the path says otherwise.

### Carried C (Korri builds it, from Moonlight Embedded `7754442` plus the plugin patches)

| Part | File and lines | What it does |
| --- | --- | --- |
| Audio choice | `io/bridge.c:21-25` | SDL audio unless X11 VA-API; then PulseAudio when a probe stream opens, else ALSA |
| ALSA output | `io/audio/alsa.c:35-94` (init), `:114-131` (play) | libopus decode, ALSA channel order, non-blocking PCM, 20 ms period, 60 ms buffer, one recover and retry |
| PulseAudio output | `io/audio/pulse.c:36-50` (probe), `:52-94` (init), `:96-107` (play) | libopus decode, ALSA map, `pa_simple` with the server's default buffer, blocking writes |
| SDL output | `io/audio/sdl.c:34-62` (init), `:81-88` (play) | libopus decode, SDL2 queue, 4096-frame device buffer, no limit on the queue |
| Picture choice and loop | `picture/bridge.c:43-54`, `:66-71`, `:74-86` | VA-API probe on X11, else SDL; the descriptor loop that Rust polls |
| X11 window and presenter thread hand-off | `picture/video/x11.c:119-131` (display, VA probe), `:133-200` (window, fullscreen, decoder), `:88-117` (present the newest frame) | Xlib window; pipe-woken presenter |
| VA-API presenter | `picture/video/ffmpeg_vaapi.c:117-160` (VA display on X11), `:233-289` (`vaPutSurface` at `:268`, EGL fallback) | Present a decoded VA surface |
| VA-API to EGL (Intel, #477) | `picture/video/vaapi_egl.c:55-116` (EGL setup), `:118-168` (dmabuf import), `:170-202` (draw, swap) | Zero-copy present where `vaPutSurface` fails under Xwayland |
| EGL software presenter | `picture/video/egl.c:90-185` (init), `:186-228` (draw) | YUV shader for a software-decoded frame in the X11 window. Unreachable: removed later on 2026-11-17 by the owner's decision (`clients/viewer/README.md`, "EGL presenter in Rust") |
| SDL window and presenter | `picture/sdl.c:58-90` (window, renderer, texture), `:92-152` (event loop, `SDL_RenderPresent` at `:141`) | The software fallback's window, vsync and present |
| SDL decode hand-off | `picture/video/sdl.c:38-90` | Decode, then an SDL event to the presenter |
| Placement | `picture/place.c:8-66` | Letterbox and the shown size (#476) |
| Frame record | `picture/frame_times.c:107-166`, called at `video/x11.c:238,242`, `video/sdl.c:64,69`, `sdl.c:142`, `ffmpeg_vaapi.c:228,273` | The measure's number 1 |
| Window input (tied to the window) | `io/input/x11.c:49-171`, `io/input/sdl.c:457-654` | Keys and mouse of the viewer's window; SDL's also has controllers |
| Video decode (not step F) | `picture/video/ffmpeg.c:49-210` | FFmpeg decoder choice and calls |

### Linked libraries (unpatched, from nixpkgs; `build.rs:94-107`, `default.nix:45-62`)

FFmpeg 9.0.1 (`libavcodec`, `libavutil`), libva 2.24.1 with `libva-x11` and the GPU's VA driver at run time, libX11 1.8.13, libglvnd 1.7.0 (EGL, GLESv2, then Mesa), SDL2 (`sdl2-compat` 2.32.72 over SDL3), libopus 1.6.1, alsa-lib 1.2.16.1, libpulseaudio 17.0 (`libpulse-simple`), plus libevdev and libudev for input. Outside step F: `moonlight-common-c` with ENet (step E), OpenSSL and libcurl (session).

ADR 0062 lets unpatched system libraries stay. Its end state removes the carried C above; its stretch goal removes the libraries too.

## 2. Replacement candidates

Crate facts are from crates.io's API on 2026-11-17 (verified); judgements are labelled.

### Audio output

| Candidate | Licence | Maintenance | What it lacks for Korri |
| --- | --- | --- | --- |
| `alsa` 0.12.1 | Apache-2.0 or MIT | Released 2026-07-31. cpal 0.18.2 depends on `alsa` ^0.11, not 0.12 (crates.io dependency list): a later cpal slice would link a second `alsa` release or move F1 to 0.11 | Still alsa-lib (allowed). Nothing else for the C's policy |
| `libpulse-simple-binding` 2.29.0 (+ `libpulse-binding` 2.30.1) | MIT or Apache-2.0 | Last releases 2025-03 and 2025-04; slow (opinion: no release in about 19 months) | Still libpulse (allowed) |
| `cpal` 0.18.2 | Apache-2.0 | Released 2026-08-16; active | A pull model (callback thread). On the fixture's ALSA `null` PCM it would loop with no wait: alsa-lib's null plugin reports a full buffer and a `/dev/null` poll descriptor (`pcm_null.c`), and cpal's ALSA worker polls, then fills what is free (`src/host/alsa/mod.rs`, `output_stream_worker`) — verified in both sources, not run. It also changes the buffering policy |
| `pulseaudio` 0.3.2 (cpal's optional Pulse backend) | MIT | Released 2026-10-16 | A native-protocol client with no libpulse; young (opinion: still 0.x, 7 releases since 2023-12-29); a second Pulse implementation to prove |
| SDL2 audio through `sdl2-sys` 0.38.0 | MIT and Zlib | 2025-07-12 | The crate carries 89 MB of SDL source (8.4 MB packed) into the vendored sources and the GPL source archive. Bindgen from `SDL.h` gives the same calls |

### Audio decode (Opus)

| Candidate | Licence | Maintenance | What it lacks |
| --- | --- | --- | --- |
| libopus 1.6.1 through Korri's bindgen | BSD-3 | nixpkgs | It is C (allowed by ADR 0062) |
| `opus` 0.4.0 | MIT or Apache-2.0 | 2026-08-23 | Its `opusic-sys` builds a libopus of its own (inferred from the dependency); Korri would then build C |
| `rusty-opus` 1.1.1 | BSD-3 | Created 2026-07-29, 11 releases, last 2026-11-14 | Its README claims bit-exact RFC 8251 vectors, multistream to 7.1 and loss concealment; not checked |
| `opus-pure` 0.2.3 | BSD-3 | 2026-08-27 to 2026-11-07 | README claims decode at 0.84–1.02× libopus's speed; not checked |
| `opuscule` 0.2.2 | MPL-2.0 | 2026-06-23 to 2026-10-20 | Decoder only, `no_std`; README claims bit-exact; not checked |
| `opus-decoder` 0.1.1, `ropus` 0.12.18, `opus-rs` 0.1.42 | MIT/Apache, BSD-3, BSD-3 | 2026 | One or two releases (`opus-decoder`), or no update since May (`ropus`) |

All the pure-Rust decoders had their first release in 2026 (verified from crates.io dates). How many authors each has was not checked. Opinion: pick by a measure, not by a README: their output against libopus's on Korri's packets, and number 3.

### Display

| Candidate | Licence | Maintenance | What it lacks for Korri |
| --- | --- | --- | --- |
| `winit` 0.30.13 | Apache-2.0 | 2026-10-24 | The window and its events. Korri's key forwarding is per Xlib keycode (`io/input/x11.c`) and SDL scancode (`io/input/sdl.c`); a winit key map to Moonlight's codes is new code. It wants to own the loop; `pump_events` fits Rust's polled loop (inferred) |
| `glutin` 0.32.3 + `glow` 0.18.0 | Apache-2.0; MIT/Apache/Zlib | 2025-04-30; 2026-07-09 | EGL context and GLES draw. No `eglCreateImageKHR` for a VA dmabuf: needs `khronos-egl` 6.0.0 (MIT/Apache, last release 2023-10-05) or bindgen |
| `wgpu` 30.0.2 | MIT or Apache-2.0 | 2026-11-12 | No import of a VA-API dmabuf without backend-specific code (inferred); a large dependency for a one-quad presenter (opinion) |
| `softbuffer` 0.4.8 | MIT or Apache-2.0 | 2025-12-13 | CPU pixels only: the SDL fallback would convert YUV on the CPU, against number 3 (inferred) |
| `x11rb` 0.14.0 | MIT or Apache-2.0 | 2026-07-16 | Pure-Rust X11 protocol, but `vaGetDisplay` and EGL's X11 platform take an Xlib `Display*` (inferred) |
| `cros-libva` 0.0.13 | BSD-3 | 2024-12-06 | DRM displays only: links `va-drm`, no `libva-x11`, no `vaPutSurface`; has `export_prime` (verified in its `build.rs` and `surface.rs`). It owns its surfaces; FFmpeg's are foreign (inferred). Bindgen of `va.h` for the three calls the EGL path uses is smaller (opinion) |
| `ffmpeg-next` 9.1.0 | WTFPL | 2026-11-11 | Decode stays FFmpeg; only needed when decode moves (not step F) |

## 3. Slices, in order

Each slice has the carried C as default and the Rust part behind a cargo feature and a run-time option, with its own package whose `bin/moonlight` chooses it, as step E did. Each is measured in turns against the six numbers on aka. Opinion, except where marked.

| Slice | What moves | Numbers it can move | Turns need | Risk |
| --- | --- | --- | --- | --- |
| F1 | The carried audio renderers and their choice (`io/audio/*`, `io/bridge.c:21-25`), same policy, over `alsa`, `libpulse-simple-binding`, bindgen of `SDL.h` and `opus_multistream.h` | 3, 4, 6 | `view --tone 1000`: without it no sound is decoded | Lowest: no window, no picture, no input; a byte-equal test against the C |
| F2 | libopus to a pure-Rust Opus decoder, inside F1's renderer | 3, 6 | `--tone`; plus a comparison of decoded PCM with libopus's | The decoders are young (opinion, from their 2026 release dates) |
| F3 | The EGL presenters (`vaapi_egl.c`, `egl.c`) to Rust (glutin or khronos-egl, glow; bindgen of three `va.h` calls); the window stays Xlib | 1, 2, 3 | `--frame-record` runs of their own; ibuki for the Intel path (#477) | GPU state and dmabuf import; the AMD `vaPutSurface` path is untouched |
| F4 | The X11 window and its key and mouse forwarding (`video/x11.c` window, `io/input/x11.c`) to winit, or x11rb with an Xlib handle for VA and EGL | 4, 5, 2 | The press of number 5 | Key codes; the window manager's fullscreen; focus under Xwayland |
| F5 | `vaPutSurface` (AMD) to the EGL path of F3, so one presenter remains | 1, 2, 3 | aka (AMD) | A policy change, not only a language change: measure it alone |
| F6 | The SDL software fallback (`picture/sdl.c`, `video/sdl.c`, its window events in `io/input/sdl.c`) to F3/F4's window and GL presenter | 1, 2, 3, 5 | A run on the SDL platform (no render node) | SDL also gives controllers; they stay until an input step |
| F7 (optional) | F1's push writes to a pull model (`cpal`), shared with Android's `Player` (`src/android/audio.rs`) | 3, 6 | A clocked null sink in the viewer fixture first, else the pull thread loops | A buffering change |

Why F1 first: it is the only part with no tie to the window, the picture, the frame record or the input; the C has three small files with no Korri patch; and a test can prove the port byte for byte against the C on the same alsa-lib.

Cost of this order: the display slices are the larger part and come later; F3 to F6 each touch the frame record's call sites (`frame_times_*`), which must move with the presenter.
