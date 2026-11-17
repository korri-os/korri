# #476 — the Linux viewer fits a new stream size, with no stretch

**Passed.** In stream-cadence's private pair on aka, korrid's resize request
turned the stream from 1280x720 to 720x1280 and back while the Linux viewer
played it on X11 VA-API. At 720x1280 the picture stood between black bars at
the sides of the 1280x720 window, at the place and scale of an aspect fit,
and the frame-ID bar rose; back at 1280x720 it filled the window again. All
24 checks passed (`result.json`), and both agents restored aka.

## The run

2026-11-17, 02:07:16 to 02:07:48 UTC, on aka, as root, with
`nix run .#viewer-linux-resize-proof -- --sunshine PACKAGE --build-store auto`
(`proof/linux_resize.py`). Other people's builds ran on aka: the heat guard
held the start for 3.5 min, until Tctl read below 85 °C. No game ran, and
the product's `korri-sunshine` had no stream. This is a proof, not a
measure.

| Part | Value |
| --- | --- |
| Viewer | This branch's `korri-viewer`, `/nix/store/4lw3f4zn3kh3wkim1lzxkncg4ylxfhkb-korri-viewer-0.1.0`, as a copied product viewer in the viewer fixture: korrid's arguments, `-platform x11_vaapi`, private Xwayland, gles2. Decode on radeonsi; frame record on |
| Sender | stream-cadence's sender fixture with `sunshine-korri` from korri-plugins `86f6236`, `/nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri` (patch set `9aefa180…`, `0046-resize-a-stream-for-korrid` last), certificate control for uid 59036 (no account). Encoder `h264_vaapi` on `renderD128`; output 1280x720 at 58.8 Hz (`sender-facts.json`) |
| korrid's request | The federation fixture's `resize`, `/nix/store/524p0cslbrhzc9h7bsg8psqsvagsy1dh-korrid-federation-fixture-0.0.0`: korrid's certificate-control adapter (`korrid::fixture_resize_stream`), as uid 59036, with Sunshine's host UUID and the viewer's client certificate |

## Results

| Step | Result | Evidence |
| --- | --- | --- |
| The picture fills the window at 1280x720 | yes | `stream-1.png`; frame IDs 490, 553, 615, 677; the bar at (64, 180), cells of 32 px |
| korrid's request for 720x1280 | `Applied` in 0.04 s | `sunshine.log` 02:07:30.496: `Runtime resolution applied: 720x1280`, `Korri certificate control: resize to 720x1280 applied` |
| **The portrait picture fits, with no stretch** | **yes** | `portrait-1.png`, `portrait-4.png`: the 720x1280 picture at x 438 to 843, 405x720; no pixel outside it above 0. Sunshine keeps its 1280x720 output's shape inside the portrait stream, so the bar has cells of 10.1 px at (457.8, 303): the bar reads there, IDs 917, 979, 1041, 1103 |
| korrid's request for 1280x720, and the picture fills the window again | `Applied` in 0.03 s; yes | `sunshine.log` 02:07:37.781; `landscape-1.png`, `landscape-4.png`; IDs 1342 to 1530 |
| The viewer reports each size it shows | yes | `viewer.log`: `picture: the picture shown is 1280x720`, `720x1280`, `1280x720` |
| The frame record | whole, in order | `result.json` `frame_record`: 1535 units, all decoded; 1533 handed to the display, in three runs of one size each (1280x720: 683, IDs 59 to 741; 720x1280: 424, IDs 742 to 1166; 1280x720: 426, IDs 1167 to 1592); no ID goes back within a size |

A stretched picture would have no bar at that place: the proof's reader
finds none in a copy of `portrait-4.png` whose picture is stretched over the
window, as `vaPutSurface` showed it before.

## What the first runs found

An earlier run of the same day, with `vaPutSurface` drawing the fitted
rectangle, failed its portrait checks: the picture in the middle was fitted,
but the bars showed the last landscape frame. Mesa 26.2's `vlVaPutSurface`
writes only its destination rectangle (`vlVaPostProc`) and presents the whole
back buffer; nothing clears around the picture. Since `626097108` a picture
with bars goes through the EGL path of #477, which clears the window first;
this run is that build. A picture that fills its window stays on
`vaPutSurface`.

## Not proven here

- A client korrid: the viewer's korrid asking the holder over the peer wire
  (`app.session.viewer.resize`) and korrid's Portrait control. The Android
  app's proof runs the peer request; korrid's unit tests run the control.
- The lifecycle report of the new `encoded` size on Linux: the fixture
  starts the viewer without a lifecycle socket. The viewer's log line comes
  from the same step that reports it (`stream.rs`); the lifecycle tests
  cover the report.
- Intel (the EGL path for every frame, #477), the SDL fallback and the X11
  software path on a device: their checks run on Xvfb in
  `checks.x86_64-linux.korri-viewer-picture`.
- What the EGL path costs on AMD for a picture with bars: no measure ran.
