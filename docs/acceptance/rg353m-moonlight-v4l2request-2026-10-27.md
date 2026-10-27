# RG353M Moonlight hardware decode through V4L2 Request

## Scope

Moonlight on the RG353M decoded H.264 in software. korrid passed
`-platform x11` to every device except the SM8550 and SM8250, and Moonlight's
`v4l2m2m` platform opens only stateful decoders through `h264_v4l2m2m`. The
RK3566 decoder is Hantro G1, a stateless decoder. This change adds a stateless
route and selects it on the RG353M.

## Device facts

Read on `haku` on 2026-10-27 with system
`/nix/store/sn7ka036qnrxi2xck60580m2d42bbsq9-nixos-system-haku-sd-card-26.11.20260923.4975466`
and Linux `7.2.0`:

| Fact | Value |
| --- | --- |
| Device-tree compatible | `anbernic,rg353p`, `rockchip,rk3566` |
| Decoder node | `/dev/video1`, `hantro-vpu` `rockchip,rk3568-vpu-dec` at `fdea0000.video-codec` |
| Decoder input formats | `S264` (H.264 parsed slices), `MG2S`, `VP8F` |
| Decoder output format | `NV12` |
| Stateless controls | `/dev/video1` exposes the H.264 decode-parameter controls |

The RKVENC encoder from the 2026-10-24 record is unrelated to this path. No
Rockchip MPP decoder driver is present, and none is needed.

## Change

- `korri-os/plugins` `services/moonlight/package.nix` builds Moonlight against
  FFmpeg 9.0.2 with LibreELEC's V4L2 Request hwaccel series
  (`LibreELEC.tv` `e9e6b8531df13bc9058ca1771dab5f0c4fd5e98e`,
  `packages/multimedia/ffmpeg/patches/v4l2-request/0001-v4l2-request.patch`),
  `--enable-v4l2-request` and `--enable-libudev`. LibreELEC master ships the
  same FFmpeg release. FFmpeg's configure reported `h264_v4l2request` enabled.
- Moonlight patch `0021-add-v4l2request-stateless-platform.patch` adds the
  `v4l2request` platform. It uses FFmpeg's native H.264 parser with the V4L2
  Request hwaccel, maps each `DRM_PRIME` frame to NV12 with `av_hwframe_map`,
  and shows it through the existing SDL NV12 presenter. It prefers H.264 only.
  The `v4l2m2m` platform is unchanged.
- korrid `services/korrid/src/linux_viewer/production.rs` selects
  `v4l2request` for device-tree `rockchip,rk3566`.

## Verification

### Decoder correctness

The patched FFmpeg decoded two libx264 High-profile clips on the RG353M with
`-hwaccel v4l2request -hwaccel_output_format drm_prime` and `hwdownload` to
NV12. Per-frame MD5s matched a host software decode exactly:

| Clip | Frames | Matching frames | Hantro interrupts |
| --- | --- | --- | --- |
| 640x480, 60 fps | 300 | 300 | 300 |
| 1280x720, 60 fps | 300 | 300 | 300 |

### Decode cost

600 frames, 10 Mb/s, High profile, no B-frames, output discarded. Two runs
each; the table shows the range.

| Clip | Hardware wall | Hardware CPU | Software, 1 thread, wall | Software, 1 thread, CPU | Software, 4 threads, wall | Software, 4 threads, CPU |
| --- | --- | --- | --- | --- | --- | --- |
| 640x480 | 1.54–1.71 s | 1.50–1.55 s | 8.53–8.60 s | 8.77–8.84 s | 3.22–4.78 s | 9.37–9.94 s |
| 1280x720 | 3.08–3.10 s | 1.66–2.38 s | 14.81–15.00 s | 14.94–15.15 s | 6.17–7.86 s | 16.97–17.50 s |

Hardware decode ran at 350–390 frames/s at 640x480 and about 195 frames/s at
1280x720. One software thread managed about 70 and 40 frames/s.

### Loopback stream

The new Moonlight binary streamed from the RG353M's own Sunshine over
`127.0.0.1` at 640x480, 60 fps and 10 Mb/s, with a temporary pairing that was
removed afterwards. Moonlight logged:

```text
v4l2m2m: setup: attached AV_HWDEVICE_TYPE_V4L2REQUEST device
Using V4L2 media driver hantro-vpu (7.2.0) for S264
v4l2m2m: submit: first frame from decoder format=drm_prime (DRM_PRIME=YES)
v4l2m2m: request: presenting DRM_PRIME frames as NV12 by map
v4l2m2m: setup: decoder=h264 (v4l2request) presenter=sdl-nv12 640x480
```

Hantro completed 60.0 decodes per second over a 30-second window in each of
three runs. The panel flipped at 60.2 per second, and a screenshot showed the
streamed picture. The loopback picture is a mirror of the screen, so it barely
changes and it cannot measure the displayed frame rate or a moving picture's
cost. Moonlight's own CPU in those runs (28–65% of one core with hardware
decode, 48–50% with software) mixes presentation through Xwayland with near-empty
frames, and is not a decode measurement.

## Limits

- The production path, korrid launching the published plugin with
  `-platform v4l2request` and evdev seats, is verified only after the plugin
  batch is published and the system is installed. See the deployment section.
- Mapping each frame for the SDL NV12 upload reads the decoder's buffer with
  the CPU. A zero-copy EGL dma-buf import is a later optimisation.
- `rockchip,rk3568` (the RG DS) has the same Hantro block but has not been
  tested, so korrid does not select `v4l2request` there.
