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

### Production stream play

haku runs system
`/nix/store/ask5rfdqg1krwnj3x2zhsv0xm0lmqarb-nixos-system-haku-sd-card-26.11.20260923.4975466`
with bundle `/nix/store/6m0hcm22vm9xbnbb1sgz9glij5pbg33w-korri-bundle-0.0.0`
and `@korri:moonlight` `/nix/store/qzza390r42x6d5hlwfkhqr9iwd9a8a6i-korri-plugin`
from batch `build-b65ce7b266b4-moonlight-v4l2request`. The RG35XX Pro held
Wario Land 4, and haku joined its invite through `app.invites.streamPlay`.

| Check | Result |
| --- | --- |
| Platform korrid passed | `-platform v4l2request` |
| Decoder | `Using V4L2 media driver hantro-vpu (7.2.0) for S264`, DRM_PRIME frames mapped to NV12 |
| Hantro decodes | 57.3 and 58.1 per second over 20–30 s windows |
| Moonlight CPU | 43.5–60.8% of one core, including audio and presentation |
| Window | Sway fullscreen, 640x480, picture correct on screen |

The first join showed the stream in a 320x480 tile beside the portal. korrid
now focuses the viewer with `[con_id=N] focus, fullscreen enable`.

Inside Moonlight, the display thread took about 37% of one core, audio about
17%, and video receive and decode submission about 8% (`top -H`, one sample).
The CPU copy of each frame for SDL presentation is the main remaining cost.

### Zero-copy presentation and Opus fix

Later on 2026-10-27, haku moved to `@korri:moonlight` from batch
`build-39fad980be48-moonlight-perf` (aarch64
`/nix/store/yr98qbqw52v7qg9s9l65gcr6iwgxs43y-korri-plugin`). That batch has
korri-plugins patch 0022, which imports Hantro's capture buffers into GLES
without a CPU copy, and builds Opus without custom modes. With custom modes,
libopus read `/proc/cpuinfo` for every audio packet.

Loopback stream on haku, 640x480 at 60 fps, two runs per build, `/proc`
samples per thread:

| Build | Moonlight | Display thread | Audio thread |
|---|---|---|---|
| SDL upload (above) | 63% | 38% | 16.6% |
| Zero-copy GL and Opus fix | 21% | 10% | 5.3% |

Production stream play, RG35XX Pro holds Wario Land 4 and haku joins its
invite, with the system built from the pin commit:

- korrid started Moonlight with `-platform v4l2request`. The log shows
  `decoder=v4l2request presenter=prime-gl` and
  `direct-dmabuf GL output=640x480 display=0 driver=x11`.
- Hantro decoded 60.0 frames per second. Moonlight used 23.2% to 30% of one
  core over 30 s and 60 s windows. korrid used 20% during the stream.
- Moonlight was fullscreen and focused, and the colours were correct in a
  screenshot of haku's output.
- The Pro ended the session, and Moonlight on haku stopped within 10 s.

With a panel powered off by `korri-display-idle`, stream play failed: the
holder's Sunshine found no monitor, and the joiner's Moonlight ignored SIGTERM
until systemd killed it after 90 s. That is issue #309. The run above powered
both panels on first.

## Deployment notes

- Installing any system built after 2026-10-24 on haku needed the one-off
  `@korri` receipt cutover (`docs/briefs/plugin-cache-pages.md`). The owner
  approved it, and the removal of three unbound `@simonwjackson` selections
  (`bitsybox-startfix`, `itchio`, `mega-man-rock-n-roll`; data kept, receipts
  backed up under `/var/lib/korri-backup-cutover-20261027T143440Z/` and
  `/var/lib/korri-backup-sj-*`).
- `@korri:moonlight` was not installed on haku. It was installed from the
  Pages cache by exact package path and approval digest.
- haku moved from owner `4994…b16e` to the owner's NIP-46 identity
  (`a80d…3dca`) with data transfer; the old identity is exported.

## Limits

- The zero-copy GL presenter still goes through X11 and GL. Showing frames on
  a DRM plane is issue #306, and it saves at most the display thread's 10%.
- `rockchip,rk3568` (the RG DS) has the same Hantro block but has not been
  tested, so korrid does not select `v4l2request` there.
