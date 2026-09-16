---
id: 01M2MAY908QX9QPSD97J6HK44X
slug: pace-rkmpp-wayland-capture-to-the-negotiated-frame-rate
title: Pace RKMPP Wayland capture to the negotiated frame rate
origin: parked
status: To Do
priority: medium
labels:
  - r36tmax
  - sunshine
  - rkmpp
  - performance
created: 2026-09-16
source: se-debug
context:
  branch: feat/r36tmax-sunshine-mpp
  repo: korri
---

# Pace RKMPP Wayland capture to the negotiated frame rate

## Why it matters

Patch 0031 made Wayland capture free-running: it keeps a screencopy request outstanding on every compositor vblank (~60.6/s) and lets the encoder discard the surplus. That was correct for the cheap validation fixture it was tuned against, but it means the full capture cost (a ~4 MB detile-and-copy per frame) is paid 60 times a second even when the client negotiates a lower rate. Lowering the stream to 30 fps therefore does not reduce compositor or memory load at all, removing an obvious mitigation for constrained content. Capture should follow the negotiated rate while still issuing each request early enough to not miss a vblank.

## Acceptance Criteria

- [ ] A 30 fps negotiated session issues ~30 screencopy requests per second, verified by VEPU interrupt rate and compositor CPU
- [ ] A 60 fps negotiated session still sustains 60.0 fps encode, matching the current measured 60.016
- [ ] Sway CPU during a 30 fps session is measurably lower than during a 60 fps session on identical content

## Related

- `services/sunshine/patches/0031-pipeline-wayland-capture-requests.patch`
- `services/sunshine/patches/0029-pace-rkmpp-after-wayland-capture.patch`

## Notes

Introduced by services/sunshine/patches/0031-pipeline-wayland-capture-requests.patch on branch feat/r36tmax-sunshine-mpp.

Pipelining itself is correct and should be kept: issuing the next request before handing the current frame to the encoder is what keeps a request outstanding at every vblank. The defect is that it never consults the target rate. A fix should sleep until shortly before the next target deadline, then issue the request, preserving the one-frame lead without capturing every vblank.

Interacts with patch 0029, which enforces the negotiated rate at the encoder with an absolute deadline. With 0031 free-running, 0029 discards the surplus after the capture cost has already been paid.
