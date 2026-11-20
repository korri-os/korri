# Research: media decoding on each Korri device, and the choice of player (#562)

Date of research: 2026-11-20. Map: #559. Labels: **verified** = I read the
primary source (a Korri file, kernel or upstream source at a named tag).
**inferred** = consistent with the sources, not proven. **opinion** = my
judgement. **unknown** = no evidence found.

## Answer

- Every Linux device that Korri ships reaches its hardware decoder only through
  V4L2. No device has a VA-API driver except x86 (ibuki). Two V4L2 kinds exist:
  - **stateless** (Request API): RG353M (RK3566 Hantro), R36T Max (RK3326/PX30
    Hantro), RG35XX (H700 Cedrus);
  - **stateful** (mem2mem): RP Mini V2 (SM8250 Iris), Odin 2 Portal and Thor
    (SM8550 Iris), Tab S6 (SM8150 Iris, out-of-tree port).
- The handhelds with Rockchip chips decode only H.264 (plus VP8 and MPEG-2) in
  hardware on mainline. HEVC and VP9 need rkvdec for RK3566, which is still
  not in Linux master (7.4-rc3, checked today).
- The Qualcomm devices decode H.264, HEVC and VP9; the SM8550 pair also
  advertises AV1.
- Android: MediaCodec, per phone. The Korri app's viewer asks MediaCodec for
  H.264 only today.
- Recommended player (opinion): **a Korri-owned player process on Linux, a
  sibling of the viewer, built on GStreamer through `gstreamer-rs`**, with
  server-side transcoding (Jellyfin) as the floor for the H.264-only
  handhelds. GStreamer is the one framework that reaches both V4L2 kinds,
  including AV1 on Iris, with no patch. libmpv is the close second: better
  player features, but it needs Korri's patched FFmpeg for stateless decode
  and cannot reach Iris AV1.
- Audio-only playback: the same player process, with no window, as a
  korrid-owned unit of the runtime user. It plays into the runtime user's
  PipeWire, the same server that games reach through
  `PULSE_SERVER=unix:/run/korri-game-audio/native`. So the two mix with no
  new audio plumbing. It must not live in the shell (another user, no access
  to that PipeWire) or in a game unit (it would stop with the game).

## Per-device hardware decode

| Device | SoC | Kernel | Decoder driver, kind | H.264 | HEVC | VP9 | AV1 | Route | Korri evidence of a decode |
|---|---|---|---|---|---|---|---|---|---|
| rg353m | Rockchip RK3566 | mainline 7.2 | `hantro-vpu` (`rockchip,rk3568-vpu`), stateless | yes, 8-bit, max 1920x1088 | no (rkvdec not in mainline, #17) | no | no | V4L2 Request | Moonlight `v4l2request`, 2026-10-27 |
| r36tmax | Rockchip RK3326 (PX30) | 7.2.6 | `hantro-vpu` (`rockchip,px30-vpu`), stateless | yes | no | no | no | V4L2 Request | 8 frames 320x240 byte-exact via GStreamer `v4l2slh264dec`, 2026-09-15 |
| rpminiv2 | Qualcomm SM8250 | 7.2 + ROCKNIX queue | `qcom-iris` (VPU2, gen1 HFI), stateful | yes | yes | yes | no | V4L2 M2M | none on record for decode (encoder only) |
| odin2portal | Qualcomm SM8550 (QCS8550) | 7.2 | `qcom-iris` (VPU3, gen2 HFI), stateful | yes | yes | yes | advertised | V4L2 M2M | Moonlight `v4l2m2m` on SM8550 Iris (legacy, July 2026) |
| thor | Qualcomm SM8550 (QCS8550) | 7.2, Odin's `.config` | `qcom-iris`, stateful | yes | yes | yes | advertised | V4L2 M2M | none on record |
| tabs6 | Qualcomm SM8150 | 7.2 + out-of-tree Iris1 "legacy VPU5" port | `qcom-iris` (port), stateful | yes | yes | yes | no | V4L2 M2M | none on record for decode (encoder for Sunshine) |
| Android app | per phone | — | MediaCodec | per phone | per phone | per phone | per phone | `AMediaCodec` (NDK) | the viewer's H.264 path (emulator proofs) |
| (ibuki, x86) | Intel UHD/Iris | — | iHD | yes | yes | yes | per GPU | VA-API | #439 |

Notes on the table:

1. **RG353M** (verified): `docs/acceptance/rg353m-moonlight-v4l2request-2026-10-27.md`
   reads `/dev/video1`, `hantro-vpu rockchip,rk3568-vpu-dec`, input formats
   `S264`, `MG2S`, `VP8F`, output `NV12`. The 1920x1088 limit and the missing
   VDPU346 rkvdec are in `docs/research/2026-11-13-viewer-rust-v4l2-decoder.md`
   §2. Today I checked that Linux `master` (7.4-rc3) `rkvdec.c` still matches
   only `rk3288`, `rk3328`, `rk3399`, `rk3588` and `rk3576`
   ([rkvdec.c](https://github.com/torvalds/linux/blob/master/drivers/media/platform/rockchip/rkvdec/rkvdec.c)).
   So #17's "enable `CONFIG_VIDEO_ROCKCHIP_RKVDEC`" cannot add HEVC on 7.2:
   no driver matches the RK3566 block (verified for the driver; the DT side is
   inferred). The kernel config comes from nixpkgs' common config through
   `nix/product/kernel/mainline-7.2.nix`; Korri adds no media symbol there.
2. **RG353M, second route** (verified, close match): Korri already carries
   Rockchip's vendor MPP service for the RKVENC encoder
   (`nix/devices/rockchip/rk-mpp-service/`). Its source has `RKVDEC2` hooks
   (`mpp_service.c`), and upstream FFmpeg has `h264_rkmpp`, `hevc_rkmpp`,
   `vp9_rkmpp` decoders
   ([rkmppdec.c](https://github.com/FFmpeg/FFmpeg/blob/master/libavcodec/rkmppdec.c)).
   HEVC on the RG353M is therefore possible through vendor C, outside V4L2.
   Nobody has built or tested that decoder port (unknown).
3. **R36T Max** (verified): `nix/devices/r36tmax/CODEC-PATH.md`. The PX30
   variant declares `HANTRO_MPEG2_DECODER | HANTRO_VP8_DECODER |
   HANTRO_H264_DECODER`; `CONFIG_VIDEO_ROCKCHIP_VDEC=m` adds nothing without a
   PX30 DT node.
4. **Iris codec lists** (verified, Linux `v7.2`):
   [`iris_platform_vpu3x.c`](https://github.com/torvalds/linux/blob/v7.2/drivers/media/platform/qcom/iris/iris_platform_vpu3x.c)
   gives `sm8550_data` the list `H264, HEVC, VP9, AV1`;
   [`iris_platform_vpu2.c`](https://github.com/torvalds/linux/blob/v7.2/drivers/media/platform/qcom/iris/iris_platform_vpu2.c)
   gives SM8250 `H264, HEVC, VP9`. The Iris Kconfig calls it a "V4L2 decoder
   driver". With Iris enabled, Venus drops its SM8250 table
   (`#if (!IS_ENABLED(CONFIG_VIDEO_QCOM_IRIS))` in
   [`venus/core.c`](https://github.com/torvalds/linux/blob/v7.2/drivers/media/platform/qcom/venus/core.c)),
   so Iris owns the Mini V2. The Mini V2 and Odin configs set both
   `VIDEO_QCOM_IRIS=m` and `VIDEO_QCOM_VENUS=m`
   (`nix/devices/rpminiv2/kernel/config`, `nix/devices/odin2portal/kernel/config`).
   AV1 on SM8550 is advertised by the driver; no one has decoded AV1 on a
   Korri device (unknown).
5. **Odin 2 Portal evidence** (verified, legacy): commit `35ce266d5` (2026-07-03)
   added Moonlight patch 0015 because "the SM8550 decoder exposes coded frame
   dimensions aligned to 128 (width) and 32 (height) without crop metadata".
   That is a working stateful decode on Linux, with a known crop trap that any
   player must also handle.
6. **Tab S6** (verified): `nix/devices/tabs6/kernel/README.md`, "Video codec":
   mainline has no SM8150 codec; the port gives `/dev/video0` as a decoder for
   H.264, HEVC, VP9. **Side finding (medium):** korrid's
   `platform_from_path` (`services/korrid/src/linux_viewer/production.rs`)
   picks `v4l2m2m` only for `qcom,sm8550` and `qcom,sm8250`. The Tab S6 says
   `qcom,sm8150`, so its Moonlight decodes in software today.
7. **Thor** (verified): DT compatible `"ayn,thor", "qcom,qcs8550", "qcom,sm8550"`
   (`nix/devices/thor/kernel/dts/qcs8550-ayn-thor.dts`); its kernel reuses the
   Odin `.config` (`nix/devices/thor/kernel/default.nix`).
8. **Android** (verified): `clients/viewer/src/android/codec.rs` picks one
   `MediaCodecList` entry "that decodes H.264"; `decoder.rs` drives
   `AMediaCodec`. The NDK also has `AMediaExtractor` (API 21) for demuxing
   files ([NDK Media](https://developer.android.com/ndk/reference/group/media)).
   The Android 12 minimum (ADR 0064) covers every call above.

## What the viewer already does

Verified from `services/korrid/src/linux_viewer/production.rs`,
`clients/viewer/picture/video/` and the acceptance records:

| Platform string | Chips | Decode | Picture |
|---|---|---|---|
| `v4l2request` | RK3566, H700 | FFmpeg 9.0.2 + LibreELEC's V4L2 Request series (not in FFmpeg upstream: master `configure` has no `v4l2_request`, checked today) | DRM_PRIME dma-buf to EGL, NV12 shader |
| `v4l2m2m` | SM8550, SM8250 | FFmpeg `h264_v4l2m2m` | `v4l2m2m.c`, crop patch 0015 |
| `x11_vaapi` | x86 with a render node | FFmpeg VA-API | `vaapi_egl.c` |
| `x11` | everything else | FFmpeg software | SDL |

The viewer decodes one elementary stream from Sunshine. It has no demuxer, no
seek, no subtitle, no A/V sync for a file, and Opus-only audio. These are the
main jobs of a media player, and the viewer does none of them (verified, by
the absence of any container code in `clients/viewer/`).

## Player options

Rules that apply (`AGENTS.md`, ADRs 0059-0064, 0087):

- no browser and no second host on a device;
- the shell is the only screen, runs as `korri-shell`, and its Slint software
  renderer cannot draw video under its widgets (ADR 0060);
- surfaces get semantic buttons only, from korrid;
- korrid performs effects; a plugin only declares;
- no compilation on a device;
- the viewer is GPL-3 (ADR 0059);
- on Android, no Korri Java (ADR 0087, #434).

| Option | Stateless V4L2 (Hantro, Cedrus) | Stateful V4L2 (Iris) incl. AV1 | Demux, seek, subtitles, audio codecs | Fit with the rules | Main cost |
|---|---|---|---|---|---|
| **GStreamer in a Korri-owned player process** (`gstreamer-rs`) | yes, unpatched `v4l2codecs` (`v4l2slh264dec`, `v4l2slh265dec`, VP8, VP9, AV1); proven by Korri on the R36T Max and the RG35XX Pro | yes, `gst-plugins-good` `v4l2` has H.264, H.265, VP9 and AV1 codec files | yes (`playbin3`, `pipewiresink`/`pulsesink`) | Same shape as the viewer (ADR 0060): own process, typed lifecycle to korrid, input from korrid | C in the decode path (LGPL); pipeline tuning per chip; Korri writes its own UI controls; subtitle and HDR tone mapping weaker than mpv (inferred) |
| **libmpv in a Korri-owned player process** | only through Korri's patched FFmpeg (`--hwdec=drm`, which needs FFmpeg's DRM hwaccel from the V4L2 Request series) | through FFmpeg's `*_v4l2m2m` wrappers, which mpv treats as copy-back decoders; FFmpeg has no `av1_v4l2m2m` | best of the four: formats, subtitles, tone mapping, render API for EGL | Same process shape; GPL-2+ build is fine beside GPL-3 | Carry the LibreELEC FFmpeg patch for a second consumer; copy-back costs memory bandwidth on handhelds (inferred); no Iris AV1 |
| **FFmpeg decode inside Korri's viewer** | same patched FFmpeg as today | `*_v4l2m2m`, no AV1 | none: Korri writes demux use, seek, A/V sync, subtitles, audio | Reuses the presenter, but grows the game-stream viewer into a media player and couples their failures | Largest Korri code; breaks the viewer's measured-stage plan (#391) with a second purpose |
| **A separate stock player binary** (for example `mpv` itself) | as libmpv | as libmpv | as libmpv | Weak: its own window, OSD and key handling; a second UI that bypasses semantic buttons and the surface | Fastest to try; wrong for the product (opinion) |

Sources for the table: [GStreamer v4l2codecs dir](https://github.com/GStreamer/gstreamer/tree/main/subprojects/gst-plugins-bad/sys/v4l2codecs),
[GStreamer v4l2 dir](https://github.com/GStreamer/gstreamer/tree/main/subprojects/gst-plugins-good/sys/v4l2),
[mpv `--hwdec`](https://github.com/mpv-player/mpv/blob/master/DOCS/man/options.rst),
[mpv `vd_lavc.c`](https://github.com/mpv-player/mpv/blob/master/video/decode/vd_lavc.c)
(wrapper decoders get `copying = true; // probably`),
[mpv Copyright](https://github.com/mpv-player/mpv/blob/master/Copyright),
[FFmpeg `v4l2_m2m_dec.c`](https://github.com/FFmpeg/FFmpeg/blob/master/libavcodec/v4l2_m2m_dec.c)
(`M2MDEC` for h264, hevc, mpeg1/2/4, h263, vc1, vp8, vp9; no av1),
Korri `nix/devices/r36tmax/CODEC-PATH.md` and
`docs/research/h700-video-engine-worksheet.md`.

Recommendation (opinion): GStreamer in a Korri-owned player process.

- Why: one unpatched framework reaches all seven Linux decoders, and Korri
  already has two byte-exact proofs with it. It brings demux and audio.
- Cost: Korri owns the pipeline choice per chip and the zero-copy path to EGL.
  It does not solve HEVC on the RG353M or the R36T Max; only transcoding does.
  The Iris crop trap (note 5) may need handling again.
- What could go wrong: GStreamer's stateful `v4l2` elements on Iris are not
  tested on a Korri device. Iris AV1 is not tested anywhere by Korri.

**Transcoding floor** (verified): Jellyfin transcodes video when "the video
codec is unsupported" by the client, and subtitles can force a burn-in
([Jellyfin codec support](https://jellyfin.org/docs/general/clients/codec-support/)).
So the player must declare per-device codecs (the table above) to the server.
The RG353M and R36T Max need H.264 at 1080p or less.

## Audio-only playback, also under a game

Verified facts (`services/inputd/nix/korri-linux-host-core.nix`,
`services/korrid/src/host/systemd_unit.rs`):

- PipeWire, `pipewire-pulse` and WirePlumber run as user services of the
  runtime user (`korri`), with linger.
- korrid runs games as units that bind `/run/user/<uid>/pulse` to
  `/run/korri-game-audio` and set `PULSE_SERVER`.
- The runtime user is in `video`, `render` and `audio`.
- The shell runs as `korri-shell`, a different user.

Consequences:

- A player unit of the runtime user plays into the same PipeWire as the game,
  and PipeWire mixes the two. No new audio path is needed (inferred).
- The player must not be in the shell: the shell has no access to that
  PipeWire, and a shell restart would stop the music.
- The player must not be in a game's unit: it would stop with the game.
- Audio-only mode needs no window, so it does not take compositor focus from
  the game (inferred).
- Still open (the map's "Background audio under a game"): volume policy or
  ducking between music and game (WirePlumber roles are one route, untested),
  and media controls in the gameplay overlay.

Android (inferred, important): music that keeps playing while another app
(a game) holds the screen needs a foreground service of type `mediaPlayback`.
An Android `Service` is a Java/Kotlin class. That conflicts with "no Java"
(ADR 0087, #434), unless a minimal service class is allowed or another route
is found. I did not verify a native-only route.

## Open questions for the map

1. Android background audio needs a `Service` class: does "no Java" allow one
   minimal class, or is background audio on Android out of scope?
2. Tab S6 (`qcom,sm8150`) is missing from korrid's `v4l2m2m` match, so
   Moonlight decodes in software there (bug-sized, separate from media).
3. HEVC/VP9 on the RG353M: wait for mainline VDPU346 rkvdec, or port the
   vendor MPP RKVDEC2 decoder beside the RKVENC port Korri already carries.
   #17 as written cannot succeed on 7.2.
4. A spike: GStreamer `playbin3` with the stateful `v4l2` decoder on the
   Odin 2 Portal (H.264, HEVC, AV1) and `v4l2codecs` on the RG353M, to the
   viewer's EGL presenter, measured with the stage-0 tool.
5. Who owns the player in the code tree (`clients/player`?, a plugin like
   `@korri:viewer`?). This is ADR-sized and not decided here.

## Sources

Korri files (all verified in this worktree at `origin/main` 7dbf77fb6):
`services/korrid/src/linux_viewer/production.rs`, `clients/viewer/README.md`,
`clients/viewer/src/android/codec.rs`, `clients/viewer/src/android/decoder.rs`,
`docs/acceptance/rg353m-moonlight-v4l2request-2026-10-27.md`,
`docs/research/2026-11-13-viewer-rust-v4l2-decoder.md`,
`docs/research/2026-11-11-x86-viewer-vaapi.md`,
`nix/devices/r36tmax/CODEC-PATH.md`, `nix/devices/tabs6/kernel/README.md`,
`nix/devices/rpminiv2/kernel/README.md`, `nix/devices/rpminiv2/kernel/config`,
`nix/devices/odin2portal/kernel/config`, `nix/devices/thor/README.md`,
`nix/devices/rockchip/README.md`, `nix/product/kernel/mainline-7.2.nix`,
`services/inputd/nix/korri-linux-host-core.nix`, ADRs 0059, 0060, 0087,
legacy commit `35ce266d5`; issues #17, #391, #439.

Upstream (all fetched and read on 2026-11-20):

- [Linux v7.2 `iris_platform_vpu3x.c`](https://github.com/torvalds/linux/blob/v7.2/drivers/media/platform/qcom/iris/iris_platform_vpu3x.c), SM8550 decode formats — checked
- [Linux v7.2 `iris_platform_vpu2.c`](https://github.com/torvalds/linux/blob/v7.2/drivers/media/platform/qcom/iris/iris_platform_vpu2.c), SM8250 decode formats — checked
- [Linux v7.2 `venus/core.c`](https://github.com/torvalds/linux/blob/v7.2/drivers/media/platform/qcom/venus/core.c), SM8250 goes to Iris — checked
- [Linux master `rkvdec.c`](https://github.com/torvalds/linux/blob/master/drivers/media/platform/rockchip/rkvdec/rkvdec.c), no RK3566/RK3568 match at 7.4-rc3 — checked
- [FFmpeg `v4l2_m2m_dec.c`](https://github.com/FFmpeg/FFmpeg/blob/master/libavcodec/v4l2_m2m_dec.c), no AV1 wrapper — checked
- [FFmpeg `configure`](https://github.com/FFmpeg/FFmpeg/blob/master/configure), no V4L2 Request — checked
- [FFmpeg `rkmppdec.c`](https://github.com/FFmpeg/FFmpeg/blob/master/libavcodec/rkmppdec.c), Rockchip MPP decoders — checked
- [GStreamer `v4l2codecs`](https://github.com/GStreamer/gstreamer/tree/main/subprojects/gst-plugins-bad/sys/v4l2codecs) and [`v4l2`](https://github.com/GStreamer/gstreamer/tree/main/subprojects/gst-plugins-good/sys/v4l2), file lists — checked
- [mpv `options.rst`](https://github.com/mpv-player/mpv/blob/master/DOCS/man/options.rst), [`vd_lavc.c`](https://github.com/mpv-player/mpv/blob/master/video/decode/vd_lavc.c), [`Copyright`](https://github.com/mpv-player/mpv/blob/master/Copyright) — checked
- [Android NDK Media](https://developer.android.com/ndk/reference/group/media), `AMediaExtractor` — checked
- [Jellyfin codec support](https://jellyfin.org/docs/general/clients/codec-support/), transcoding triggers — checked

Not checked: the Android foreground-service rule (from memory, labelled
inferred); GStreamer and mpv performance on any Korri handheld; Iris AV1 on
real hardware.
