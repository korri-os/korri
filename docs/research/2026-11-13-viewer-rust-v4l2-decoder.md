# Research: a Rust V4L2 stateless H.264/HEVC decoder for RK3566 and Allwinner H700 (step I of korri-os/korri #391)

Date of research: 2026-11-13. Labels: **verified** = I read the primary source. **inferred** = consistent with the sources, not proven. **unknown** = no evidence found. **opinion** = my judgement.

Source: a researcher agent's brief for #391, step I. Its claims were not audited independently; each carries its own label.

## Summary

No Rust project today decodes H.264 or HEVC through V4L2 stateless on both of Korri's chips. Two Rust pieces exist:
- `v4l2r` (MIT, active, 0.0.8 of 2026-07-30) gives the ioctl layer. That includes Media Request alloc, queue and reinit, and typed H.264 controls.
- `cros-codecs` (BSD-3) has an H.264 V4L2 stateless backend. That backend works only on multi-planar, frame-based, Annex-B drivers, so it fits Hantro and not Cedrus. It has no HEVC V4L2 backend. Its AOSP upstream has had no commit since about March 2025 (verified).

GStreamer's unpatched C `v4l2codecs` plugin covers all three drivers, and Rust can drive it through `gstreamer-rs`. It is the close match that meets "no C that Korri writes or patches", but it leaves C in the decode path.

The kernel limits the codec choice:
- **RG353M** (mainline Linux 7.2): the only decoder is Hantro VDPU2. It does H.264 8-bit up to 1920x1088 and no HEVC. Mainline has no RK3566/RK3568 rkvdec ("VDPU346"); that series is at v3 and unmerged (verified).
- **RG35XX Pro**: Cedrus runs only on Korri's patched Linux 7.2 (the H616 VE series plus a no-IOMMU CMA pool). It is slice-based, takes no start code, uses the single-planar API, and outputs linear NV12. H.264 is proven byte-exact on the device. HEVC is not established (verified).

Recommended route (opinion): a Korri-owned Rust crate.
- It is H.264-only and built on `v4l2r`.
- It reuses `cros-codecs`' BSD-3 H.264 parser and DPB as vendored code.
- It has two backends: Hantro (multi-planar, frame-based, Annex-B) and Cedrus (single-planar, slice-based, no start code).
- It exports linear NV12 as DMA-BUF into the presenter that step G leaves in place.
- Size: about 3–5 kLOC of new Rust plus about 5–7 kLOC vendored (opinion).

The main risks:
- Korri owns H.264 DPB correctness.
- The H700 decoder depends on Korri's kernel patches, and the upstream Linux 7.4 DT puts back the IOMMU that Korri removed.
- No handheld baseline exists yet to judge against.

The stage-0 measure (`stream-cadence view` and `verdict`) would judge the new decoder against handheld baselines that step G must first record. The judge uses ADR 0062's numbers. "Equal" means inside the run-to-run spread of five runs.

**Correction to #391's evidence (loud):** #391 says "`cros-codecs` is abandoned and never finished H.264 or HEVC for it". That is half right.
- An H.264 V4L2 stateless backend exists and is wired end to end. Commit notes say Google tested it on MediaTek "corsola". It does not work for Cedrus.
- HEVC V4L2 does not exist.
- "Abandoned" overstates it. AOSP `main` is dormant since about March 2025, and the GitHub mirror merged one small commit on 2026-10-21.
- Likely cause (inferred): the earlier pass relied on the crate's README, which still lists "Stateless V4L2 decoder support" under "Planned features" (verified).

## Findings

### 1. Rust projects that drive V4L2 stateless decoders

| Project | Licence | Stateless codecs (V4L2) | Driver fit: Hantro / rkvdec / rkvdec2 / Cedrus | DMA-BUF | Maturity | Maintainer |
|---|---|---|---|---|---|---|
| `cros-codecs` 0.0.6 (crates.io 2025-06-18); GitHub mirror of AOSP `platform/system/cros-codecs` | BSD-3-Clause (verified) | H.264, VP8, VP9, AV1 (verified). H.265: VA-API only, no V4L2 file (verified) | Multi-planar only, with frame-based and Annex-B hard-coded. So Hantro and rkvdec are plausible and Cedrus is excluded (code verified; driver fit inferred) | Imports external DMA-BUF/GBM frames into CAPTURE (code verified) | Prototype (verified):<br>• `todo!()` for field pictures and for `is_ready`<br>• `panic!("handle this better")`<br>• sleep-based polling with back-off<br>• decoded format hard-coded to MediaTek `MM21`<br>• "TODO: handle video formats other than h264" | Google ChromeOS/Android media team (Fritz Koenig, Hang Nguyen and Justin Green in the log). AOSP `main` last commit about 1 year 8 months ago (verified) |
| `v4l2r` 0.0.8 (2026-07-30) | MIT (verified, LICENSE text) | No decoder logic. The ioctl layer has:<br>• `MEDIA_IOC_REQUEST_ALLOC`, `MEDIA_REQUEST_IOC_QUEUE` and `REINIT`<br>• request-scoped ext controls<br>• typed H.264 stateless controls (SPS, PPS, scaling, decode and slice params, pred weights, decode mode, start code)<br>• VP8 and VP9<br>No typed HEVC stateless controls (verified) | Driver-neutral. Handles single- and multi-planar queues (README; verified) | MMAP, USERPTR, DMABUF import, `VIDIOC_EXPBUF` export (verified file list) | Active: commits in 2026-06, 2026-07 and 2026-10 (verified). Its high-level decoder is stateful only (verified) | Alexandre Courbot (verified) |
| `gst-plugins-rs` | per plugin | **None.** Its video decoders are dav1d, ffv1, gif, vvdec, webp and cdg (verified README) | — | — | — | GStreamer project |
| GStreamer `v4l2codecs` (C, gst-plugins-bad) via `gstreamer-rs` 0.25.4 | Plugin: LGPL (inferred from gst-plugins-bad). `gstreamer-rs`: MIT OR Apache-2.0 (verified) | `v4l2slh264dec`, `v4l2slh265dec`, MPEG-2, VP8, VP9, AV1 (verified docs) | Generic: it generates elements from the devices present (verified docs). Cedrus H.264 is proven on Korri's RG35XX Pro with `v4l2slh264dec`, byte-identical to the reference (verified, Korri worksheet). Hantro and rkvdec: inferred | `src` caps `video/x-raw(memory:DMABuf), format=DMA_DRM`, plus NV12, NV12_32L32 and others (verified) | Upstream since GStreamer 1.18 (not verified). Korri ran it from nixpkgs GStreamer 1.26.5 (verified) | Author Nicolas Dufresne (Collabora) (verified). Rust bindings: Sebastian Dröge (verified) |
| `linux-video-rs` (katyo) | unknown | Stateless control types, but no request-fd wrapper (search only) | — | — | — | search only |
| `shiguredo/v4l2-rs` | unknown | Stateful bcm2835 only (search only) | Not stateless | — | — | search only |

I searched these places and found nothing that drives a stateless decoder:
- crates.io and docs.rs for `v4l2r`, `cros-codecs`, `linux-video`, `v4l2-sys` and `v4l`;
- GitHub, for Rust code that uses `MEDIA_IOC_REQUEST_ALLOC` or `V4L2_PIX_FMT_H264_SLICE`;
- the `gst-plugins-rs` plugin list.

Close matches outside Rust each break the "Rust" preference:
- Bootlin `v4l2-request-test` (C);
- FFmpeg's unmerged series (C);
- Rockchip MPP (a vendor C library on the vendor kernel, not V4L2).

1. **Claim:** `cros-codecs`' V4L2 H.264 backend always sets `V4L2CtrlH264DecodeMode::FrameBased` and `V4L2CtrlH264StartCode::AnnexB`. It sends no `SLICE_PARAMS` or `PRED_WEIGHTS` controls. **Sources:** [cros-codecs h264/v4l2.rs](https://github.com/chromeos/cros-codecs). **Support:** direct evidence (source read). **Confidence:** high.
2. **Claim:** `cros-codecs`' device discovery skips any video node that has no multi-planar OUTPUT queue (`Queue::get_output_mplane_queue`). It takes the first match and does not check for the codec. Cedrus registers single-planar queues (`V4L2_BUF_TYPE_VIDEO_OUTPUT`). So `cros-codecs` cannot open Cedrus as written. **Sources:** cros-codecs `src/device/v4l2/utils.rs`; [Linux cedrus_video.c](https://github.com/torvalds/linux/blob/master/drivers/staging/media/sunxi/cedrus/cedrus_video.c). **Support:** direct evidence for both facts; the conclusion is interpretation. **Confidence:** high.
3. **Claim:** `cros-codecs`' H.264 parser already computes `dec_ref_pic_marking_bit_size` and `pic_order_cnt_bit_size`. Korri's FFmpeg patch had to add these two fields to FFmpeg's H.264 parser. **Sources:** cros-codecs `controls/h264.rs`; Korri `ffmpeg/0001-v4l2-request.patch` (patch 06/13). **Support:** direct evidence. **Confidence:** high.
4. **Claim:** `cros-codecs` 0.0.6 is 36,068 Rust code lines in total. `v4l2r` 0.0.8 is 8,677 lines. **Sources:** [crates.io cros-codecs](https://crates.io/api/v1/crates/cros-codecs), [crates.io v4l2r](https://crates.io/api/v1/crates/v4l2r). **Support:** direct evidence. **Confidence:** high.

### 2. What each chip's kernel driver exposes

Note: Korri runs Linux **7.2** on both devices, not 6.x (verified: `nix/devices/rg353m/sd-image.nix`, `nix/devices/h700/README.md`).

| | RG353M (RK3566) | RG35XX Pro (H700, H616 family) |
|---|---|---|
| Kernel | Mainline 7.2 (verified) | 7.2 with the ROCKNIX H700 queue, Chen-Yu Tsai's v3 H616 VE series (Korri patches 0224–0230), and Korri patch 0236: no IOMMU, 64 MiB reusable CMA `ve-pool` (verified) |
| Decoder driver | `hantro-vpu`, compatible `rockchip,rk3568-vpu` = VDPU2: H.264, MPEG-2, VP8; NV12 out; H.264 max 1920x1088 (`FMT_FHD`) (verified) | `cedrus` (staging). H616 uses the H6 variant: MPEG-2, H.264, H.265, H.265 10-bit, VP8 (verified) |
| HEVC | **None in mainline.** `rkvdec` matches only rk3288, rk3328, rk3399, rk3588 (VDPU381) and rk3576 (VDPU383) (verified). VDPU346 for RK3566/68 is at v3 (2026-01-10). It depends on Detlev Casanova's VDPU381/383 series and is not in `master` (verified) | The driver advertises it (verified). Korri has not tested it: "Not established: … HEVC" (verified) |
| H.264 decode mode / start code | `FRAME_BASED` / `ANNEX_B` only (verified) | `SLICE_BASED` / `START_CODE_NONE` (verified) |
| Queue API | Multi-planar (verified) | Single-planar; `requires_requests = true`; `HOLD_CAPTURE_BUF` for H.264 and HEVC (verified) |
| Capture formats | NV12 (verified) | NV12 linear first, then NV12_32L32 (tiled), NV21, YUV420, YVU420. NV12 stride and height are aligned to 16 (verified) |
| HEVC limits | — | `SLICE_PARAMS` dims {1} (one slice per request); up to 256 entry points (verified) |
| Other limits | 8-bit, 4:2:0 or 4:0:0 only; Baseline to High (verified) | max 4096x2304 (verified) |
| Memory | `vb2_dma_contig`, MMAP or DMABUF (verified) | `vb2_dma_contig`, MMAP or DMABUF; physical addresses from `ve-pool`, no IOMMU (verified) |
| Device nodes on Korri | `/dev/video0` RGA, `/dev/video1` Hantro decoder, `/dev/video2` Hantro encoder (Korri `gpu.nix` comment; verified as Korri's record) | `/dev/video0` decoder, `/dev/video1` Korri's stateful H.264 encoder (verified) |

5. **Claim:** H616 Cedrus support (the driver compatible) is in Linux `master`. The DT nodes went to `sunxi/dt-for-7.4` on 2026-10-06. Patch 7 adds the H616 VE "with fixed IOMMU ports". **Sources:** [cedrus.c master](https://github.com/torvalds/linux/blob/master/drivers/staging/media/sunxi/cedrus/cedrus.c); [Chen-Yu Tsai, applied to sunxi/dt-for-7.4](https://lists.openwall.net/linux-kernel/2026/10/05/1940). **Support:** direct evidence. **Confidence:** high.
6. **Claim:** Korri's working H700 decode depends on removing the VE's IOMMU link (patch 0236). With the IOMMU on, the display engine faulted on panel wake. The 7.4 upstream DT puts the IOMMU back. A move to the upstream DT therefore brings the display fault back unless someone fixes it. **Sources:** Korri `docs/research/h700-video-engine-worksheet.md` §5–6; openwall link above. **Support:** direct evidence for both facts; the conflict is interpretation. **Confidence:** medium.
7. **Claim:** Korri proved H.264 hardware decode on the RG35XX Pro through GStreamer `v4l2slh264dec`:
   - 120 frames at 640x480 decoded byte-identical to an FFmpeg software reference (SHA-256 `0c891fbb…`);
   - 120 requests and 120 VE interrupts;
   - across 3 boots.
   
   Not established: sustained 60 fps, other profiles, HEVC. **Sources:** worksheet §2 and §6. **Support:** direct evidence. **Confidence:** high.
8. **Claim:** The FFmpeg V4L2 Request hwaccel is still not in FFmpeg upstream. `configure` on `master` has no `v4l2_request`, `v4l2request` or `v4l2-request` string (checked 2026-11-13). **Sources:** [FFmpeg configure (GitHub mirror)](https://github.com/FFmpeg/FFmpeg/blob/master/configure). **Support:** direct evidence. **Confidence:** high. The GitHub mirror can lag git.ffmpeg.org by hours, not years.

**Workarounds visible in Korri's FFmpeg patch (verified; read in full except the VP9 hunk).** The 13 patches are the series by Jonas Karlman, Jernej Skrabec and Boris Brezillon, as LibreELEC ships it. I saw no Korri-authored hunk. The series makes these accommodations for drivers:
- **Device probe.** It walks every `/dev/media*` through udev. It requires `V4L2_BUF_CAP_SUPPORTS_REQUESTS`, the coded format and the frame size. It handles single- and multi-planar queues.
- **Decode mode and start code.** H.264 and HEVC read both from the driver's **default control value**. This is how one code path serves Hantro and rkvdec (frame-based, Annex-B) and Cedrus (slice-based, no start code). The series prepends `00 00 01` only for Annex-B.
- **Slice-based decode.** It uses `V4L2_BUF_FLAG_M2M_HOLD_CAPTURE_BUF`. When a driver lacks that flag, the series queues each slice as a frame.
- **HEVC arrays.** HEVC queries the array dimensions of `SLICE_PARAMS` and `ENTRY_POINT_OFFSETS`. It splits a frame into several requests when they overflow; Cedrus allows 1 slice per request.
- **References.** A reference frame is named by its timestamp, which is the CAPTURE index + 1 µs.
- **Buffers.**
  - 4 OUTPUT buffers and requests in a ring. OUTPUT size is max(w·h·3/2, 256 KiB).
  - CAPTURE buffers are created one at a time and exported O_RDONLY through `VIDIOC_EXPBUF`.
  - Polls time out at 2 s.
- **Capture format.** It takes the driver default first, else the first known format of matching bit depth. The table maps `NV12_32L32` to `DRM_FORMAT_MOD_ALLWINNER_TILED`. It adds AFBC 16x16 split "used by Cedrus" (patch 12, Dec 2025) and Broadcom SAND128.
- **AFBC.** Mainline Cedrus has no AFBC capture format (verified, `cedrus_video.c`). Inferred: that patch serves an out-of-tree Cedrus.

**Korri's own workarounds live in the Moonlight patches** (verified):
- 0015 crops the coded alignment padding.
- 0021 adds the platform and prefers H.264 only.
- 0022 adds the zero-copy presenter. See §3.

### 3. Display side

9. **Claim:** What Moonlight's `v4l2request` path does today (patches 0021 and 0022; verified):
   - **Decode setup.**
     - FFmpeg's software `h264` or `hevc` decoder runs with the V4L2 Request hwaccel.
     - It uses `LOW_DELAY`, `OUTPUT_CORRUPT`, `SHOW_ALL` and 1 thread, and outputs `AV_PIX_FMT_DRM_PRIME`.
     - It refuses a software fallback.
   - **Picture path.**
     - A display thread imports each capture buffer's Y plane as `DRM_FORMAT_R8` and its UV plane as `DRM_FORMAT_GR88`. Each plane becomes one EGLImage through `EGL_EXT_image_dma_buf_import`, in SDL's GLES 3.0 context.
     - It caches up to 32 imports, keyed by dma-buf fd.
     - It draws a full-screen quad with an NV12 shader. The shader takes the matrix and range from the frame, else from the host's colorspace for each decode unit.
     - It accepts only one-layer linear NV12 (modifier LINEAR or INVALID).
     - It keeps the last shown frame referenced, so the decoder cannot reuse a buffer that the GPU still reads. Then it calls `SDL_GL_SwapWindow`.
   - **Timing.**
     - When the display lags, it drops the pending frame (single-slot mailbox).
     - It creates the window on the display thread. EGL bring-up on H700 takes 9–12 s, which is longer than Sunshine's first-ping wait.
   - **Window.**
     - It forces EGL under X11 (`SDL_HINT_VIDEO_X11_FORCE_EGL`).
     - It goes fullscreen-desktop.

   **Sources:** Korri patches 0021 and 0022. **Support:** direct evidence. **Confidence:** high.
10. **Claim:** On both devices, the viewer is an X11 client (`DISPLAY=:0`, `XDG_SESSION_TYPE=x11`) of Korri's sway. The compositor owns KMS (`backend = "drm"`, `renderer = "gles2"`, panel `DSI-1` 640x480@60). On the RG35XX, the compositor is sway with wlroots 0.20.2. So the path today is:
    1. decoder dma-buf;
    2. viewer GLES pass;
    3. Xwayland;
    4. sway GLES composition;
    5. primary plane.

    **Sources:** `nix/devices/rg353m/hardware.nix`, `nix/devices/h700/board.nix`, worksheet §6. **Support:** direct evidence for the configuration. The chain through Xwayland is inference. That the RG353M runs sway is inferred from the shared `korriLinuxHost.compositor` module. **Confidence:** medium.
11. **Claim:** Direct KMS from the viewer needs DRM master. The vendored PR #932 `ffmpeg_drm` backend works this way: it makes atomic commits on an overlay plane and blanks the other planes. sway holds DRM master. korrid never selects that platform for these chips; it picks `v4l2request`. **Sources:** Korri `0001-vendored-ffmpeg-drm-prime-pr932.patch`; `services/korrid/src/linux_viewer/production.rs`. **Support:** direct evidence for both facts. That it cannot coexist with sway is inference. **Confidence:** high.
12. **Claim:** A Wayland-native viewer can give the decoder's NV12 dma-buf to sway through `zwp_linux_dmabuf_v1`.
    - `smithay-client-toolkit` 0.21.1 (MIT) has `DmabufState`, `DmabufParams` and feedback tranches.
    - sway either composites the buffer on the GPU or scans it out directly on the **primary** plane.
    - wlroots puts surfaces on overlay planes only through `wlr_output_layer`. Callers must turn that off for screen capture.

    **Sources:** [sct dmabuf docs](https://docs.rs/smithay-client-toolkit/latest/smithay_client_toolkit/dmabuf/index.html), [wlroots wlr_output_layer.h](https://wlroots-baa023.pages.freedesktop.org/wlr/types/wlr_output_layer.h.html). **Support:** direct evidence for the APIs. That sway does not drive output layers by default is search-level evidence. **Confidence:** medium.
13. **Claim:** On RK3566, VOP2 has three kinds of window:
    - Esmart0 and Esmart1 are **primary**-type planes that take linear NV12.
    - The Smart windows are RGB-only.
    - The Cluster windows take YUV only with AFBC.

    So direct scanout of Hantro's linear NV12 is possible on the RG353M **if** sway's CRTC uses an Esmart window as its primary plane. That assignment is unknown. The H700 (DE33) planes are unknown. **Sources:** [rockchip_vop2_reg.c](https://github.com/torvalds/linux/blob/master/drivers/gpu/drm/rockchip/rockchip_vop2_reg.c). **Support:** direct evidence for the formats; the plane assignment is unknown. **Confidence:** medium.
14. **Claim:** Rust crates exist for the display side:
    - `smithay-client-toolkit` and `wayland-client` for Wayland dmabuf;
    - `drm` for KMS;
    - `khronos-egl` with `glow` for EGL import and GLES.

    **Sources:** crates.io for sct (fetched); the others are search-level. **Support:** interpretation. **Confidence:** medium.

### 4. Routes, size, risks, and how stage 0 judges them

| Route | What Korri writes | C left in the decode path | Fits both chips? | Size (opinion) | Main risk |
|---|---|---|---|---|---|
| **B (recommended).** Korri crate on `v4l2r` plus a vendored `cros-codecs` H.264 parser and DPB | Request backends for Hantro VDPU2 (multi-planar, frame, Annex-B) and Cedrus (single-planar, slice, no start code, slice params and pred weights). Also: poll-based completion, a probe that checks the codec, NV12 negotiation, EXPBUF export | None (kernel only) | Yes, for H.264. HEVC is impossible on RG353M mainline anyway | ~3–5 kLOC new + ~5–7 kLOC vendored (parser and DPB) | Korri owns H.264 DPB correctness: MMCO, long-term refs, errors that must request an IDR. No upstream maintainer |
| A. `gstreamer-rs` + unpatched `v4l2slh264dec` through `appsrc`/`appsink` | Pipeline glue; DMA_DRM caps passed to the presenter | GStreamer, GLib, gst-plugins-bad. All unpatched; ADR 0062 allows "unpatched system libraries" | Yes (Cedrus proven by Korri) | ~1–2 kLOC | Latency: needs a live upstream and `compliance=normal` or `flexible`, else the DPB can fill before output (verified docs). Large C closure. Stretch goal unmet. #391 asks that this path "move to Rust" |
| C. Use `cros-codecs` as a dependency and fix it upstream | Same fixes as B, sent upstream | None | After the fixes | Like B | Upstream dormant since about 2025-03, with a MediaTek and Android focus |
| D. Wait for FFmpeg upstream | Nothing | FFmpeg | Yes | 0 | Unmerged since 2024-08. Still C. Does not meet the end goal |

15. **Claim (opinion):** Choose B and keep it H.264-only. Reasons:
    - The end goal is a Rust path.
    - Korri needs only 8-bit 4:2:0 progressive H.264 at the panel's 640x480, so field and interlace code can stay out.
    - RK3566 has no mainline HEVC.
    - `cros-codecs` already supplies a parser that computes the hard bit-size fields.
    - Korri has a ready conformance fixture with a reference SHA on the H700.

    Keep A as the fallback if B fails "equal".

    **Cost:**
    - B is the largest new code in #391 and gives no speed by itself, as #391 says.
    - Sunshine's encoders differ (NVENC, AMF, QSV, VA-API, x264). B needs a conformance run for each encoder, for example Fluster JVT-AVC through a CLI like `ccdec`.
    - The H700 path stays tied to Korri's 7.2 kernel patches until the IOMMU and display fault is fixed upstream.

    **Support:** interpretation. **Confidence:** medium.
16. **Claim:** Change one part at a time. Step I should swap only the decoder under the presenter that step G leaves in place, so each change in the numbers has one cause. ADR 0062 says: "A stage that changes one part gives each change in the numbers one cause". Do not combine the decoder swap with a move from EGL import to Wayland dmabuf or to direct scanout. **Sources:** `docs/adr/0062-the-viewer-moves-to-rust-in-measured-stages.md`. **Support:** direct evidence for the rule; applying it here is interpretation. **Confidence:** high.
17. **Claim:** How the stage-0 measure judges step I:
    - **The tool.** `nix/stream-cadence` `view` runs a private pair:
      - a sender fixture with a frame-ID source and stock Sunshine on its own ports;
      - the installed viewer on the device's own compositor.

      A wlr-screencopy client reads the frame-ID bar from every output frame. It stamps each frame with the compositor's commit time, not panel scanout.
    - **The verdict.** `verdict BASELINE CANDIDATE` gives equal, better or worse for each number.
      - Equal means inside the run-to-run spread.
      - Worse or better needs two non-overlapping sets of five runs.
      - It refuses to compare across different sender conditions, across a live game, or across a load difference of 5 points of all CPUs or more.
    - **What it records.**
      - Which decode ran. The run ends if hardware decode did not start.
      - Sender presentation to display, p50, for the base and queue measures.
      - Dropped, repeated and late frames; torn or unreadable frames.
      - Viewer CPU seconds each second, and other load.
      - Startup, stalls and exits.
    - **What it does not measure yet.**
      - Packet arrival to display (number 1) for today's viewer.
      - Button to packet (number 5).
      - Battery.
    - **What blocks it.**
      - No handheld baseline exists.
      - The RG35XX Pro has one valid short run (1,955 records at 60.0 each second).
      - The RG353M run was invalid (11 gaps), and the device is off limits.
      - Handheld baselines wait for step G.

    **Sources:** [#432](https://github.com/korri-os/korri/issues/432), [#391](https://github.com/korri-os/korri/issues/391), ADR 0062. **Support:** direct evidence. **Confidence:** high.
18. **Claim (inferred):** Three properties of the measure matter for this route:
    - Screencopy needs the full output composited, so the measure does not observe direct scanout or overlay offload. Keep the presenter fixed (finding 16).
    - Decode corruption shows up as unreadable frame IDs. A silent reference error that leaves the bar readable does not. Add a byte-exact fixture test beside the measure.
    - "Viewer CPU" counts the viewer process only. Work moved into sway shows only in "other load".

    **Support:** interpretation. **Confidence:** medium.

## Contradictions

- **cros-codecs status.** #391 says `cros-codecs` "never finished H.264 … for it" and is "abandoned". The source shows a working H.264 V4L2 backend (multi-planar, frame-based only) and a dormant, not archived, upstream. The GitHub mirror merged a commit on 2026-10-21. AOSP `main` shows nothing newer than about 1 year 8 months.
- **HEVC V4L2 in cros-codecs.** One search summary said HEVC V4L2 modules (`h265/v4l2.rs`, `controls/h265.rs`) existed in ChromeOS `platform2`'s copy before its removal on 2025-10-01. The AOSP/GitHub tree I read has no H.265 V4L2 file. Unresolved: the `platform2` page fetch returned the repository README, not the commit.
- **Decoder name.** korrid's comment calls the RK3566 decoder "Hantro G1". Mainline binds `rockchip,rk3568-vpu` to the VDPU2 register set (`rockchip_vdpu2_dec_fmts`). This is only a naming difference; the capabilities in §2 come from the driver.
- **AFBC.** The Korri FFmpeg patch carries AFBC for Cedrus. Mainline Cedrus offers no AFBC capture format.

## Missing evidence

- The exact sway and Xwayland buffer path, and whether sway direct-scans the viewer today. This needs `WAYLAND_DEBUG` or sway debug logs on the device.
- `drm_info` from both devices: which plane is primary on each CRTC, and its NV12 modifiers. DE33 on H700 is unknown.
- Whether Sunshine's streams to these devices use multiple slices per frame, long-term references, or other H.264 features that stress Cedrus slice mode and the DPB.
- Cedrus HEVC, and sustained 60 fps H.264, on the RG35XX Pro.
- Decode time per frame on Hantro and Cedrus with today's FFmpeg path. Korri has recorded no per-frame numbers.
- Whether GStreamer `v4l2slh264dec` with `compliance` set and a live `appsrc` matches FFmpeg's `LOW_DELAY` output latency (route A).
- GPL-3 compatibility of BSD-3, MIT, Apache-2.0 and LGPL-2.1+. It is standard, but I did not re-check it against ADR 0059's text.
- When the VDPU346 (RK3566 rkvdec) series will merge.

## Sources

Kept:
- [Korri moonlight `package.nix`](file:///home/simonwjackson/code/sandbox/korri-plugins/services/moonlight/package.nix) — FFmpeg patch provenance (LibreELEC e9e6b853), patch list — fetched
- Korri `patches/ffmpeg/0001-v4l2-request.patch` (6,439 lines, 13 patches) — driver accommodations — fetched
- Korri `patches/0021-add-v4l2request-stateless-platform.patch`, `0022-v4l2request-zero-copy-gl-background-window.patch`, `0015-crop-coded-alignment-padding-on-present.patch`, `0001-vendored-ffmpeg-drm-prime-pr932.patch` — today's picture path — fetched
- Korri `services/korrid/src/linux_viewer/production.rs` — platform choice per device tree — fetched
- Korri `nix/devices/h700/README.md`, `nix/devices/h700/board.nix`, `nix/devices/rg353m/{hardware,sd-image,gpu}.nix`, `docs/research/h700-video-engine-worksheet.md`, `docs/adr/0062-the-viewer-moves-to-rust-in-measured-stages.md` — kernels, compositor, decode proof, measure rule — fetched
- [Epic: one Korri viewer in Rust … (#391)](https://github.com/korri-os/korri/issues/391) — stages, step I, earlier claims — fetched
- [Viewer stage 0: measure today's viewers … (#432)](https://github.com/korri-os/korri/issues/432) — measure method, baselines, handheld status — fetched
- [chromeos/cros-codecs](https://github.com/chromeos/cros-codecs) — source read (README, Cargo.toml, h264/v4l2.rs, backend, device, utils, queue, controls) — fetched
- [platform/system/cros-codecs log (AOSP)](https://android.googlesource.com/platform/system/cros-codecs/+log/refs/heads/main) — last activity — fetched
- [GitHub API commits, chromeos/cros-codecs](https://api.github.com/repos/chromeos/cros-codecs/commits?per_page=8) and [Gnurou/v4l2r](https://api.github.com/repos/Gnurou/v4l2r/commits?per_page=5) — dates — fetched
- [Gnurou/v4l2r](https://github.com/Gnurou/v4l2r) — request ioctls, controls, licence — fetched
- [crates.io: cros-codecs](https://crates.io/api/v1/crates/cros-codecs), [v4l2r](https://crates.io/api/v1/crates/v4l2r), [gstreamer](https://crates.io/api/v1/crates/gstreamer), [smithay-client-toolkit](https://crates.io/api/v1/crates/smithay-client-toolkit) — versions, licences, line counts — fetched
- [Linux cedrus.c](https://github.com/torvalds/linux/blob/master/drivers/staging/media/sunxi/cedrus/cedrus.c), [cedrus_video.c](https://github.com/torvalds/linux/blob/master/drivers/staging/media/sunxi/cedrus/cedrus_video.c) — variants, controls, formats — fetched
- [Linux hantro_drv.c](https://github.com/torvalds/linux/blob/master/drivers/media/platform/verisilicon/hantro_drv.c), [rockchip_vpu_hw.c](https://github.com/torvalds/linux/blob/master/drivers/media/platform/verisilicon/rockchip_vpu_hw.c) — RK3568 VDPU2 capabilities — fetched
- [Linux rkvdec.c](https://github.com/torvalds/linux/blob/master/drivers/media/platform/rockchip/rkvdec/rkvdec.c) — compatibles; no RK3568 — fetched
- [Linux rockchip_vop2_reg.c](https://github.com/torvalds/linux/blob/master/drivers/gpu/drm/rockchip/rockchip_vop2_reg.c) — RK3568 plane formats — fetched
- [[PATCH v3 0/3] media: rockchip: rkvdec: add support for the VDPU346 variant](https://lkml.rescloud.iu.edu/hypermail/linux/kernel/2601.1/07308.html) — RK356x rkvdec status — fetched
- [Re: (subset) [PATCH v3 0/7] arm64: allwinner: h616: Support Video Engine](https://lists.openwall.net/linux-kernel/2026/10/05/1940) — DT applied for 7.4 — fetched
- [FFmpeg configure (master)](https://github.com/FFmpeg/FFmpeg/blob/master/configure) — no v4l2request — fetched
- [GStreamer v4l2slh264dec](https://gstreamer.freedesktop.org/documentation/v4l2codecs/v4l2slh264dec.html), [v4l2codecs](https://gstreamer.freedesktop.org/documentation/v4l2codecs/index.html), [GstH264Decoder](https://gstreamer.freedesktop.org/documentation/codecs/gsth264decoder.html) — elements, caps, compliance — fetched
- [gst-plugins-rs README](https://github.com/GStreamer/gst-plugins-rs/blob/main/README.md) — no V4L2 decoder — fetched
- [smithay_client_toolkit::dmabuf](https://docs.rs/smithay-client-toolkit/latest/smithay_client_toolkit/dmabuf/index.html), [wlroots wlr_output_layer.h](https://wlroots-baa023.pages.freedesktop.org/wlr/types/wlr_output_layer.h.html) — display APIs — fetched

Search only:
- [katyo/linux-video-rs](https://github.com/katyo/linux-video-rs)
- [shiguredo/v4l2-rs](https://github.com/shiguredo/v4l2-rs)
- [bootlin/v4l2-request-test](https://github.com/bootlin/v4l2-request-test)
- [FFmpeg-devel v2 V4L2 Request series (2024-08)](https://ffmpeg.org/pipermail/ffmpeg-devel/2024-August/332034.html)
- [Frigate RK3588 discussion](https://github.com/blakeblackshear/frigate/discussions/18311)
- [sway issue 9220](https://github.com/swaywm/sway/issues/9220)
- [Porthole: wlroots DRM layers need libliftoff](https://porthole-dev.github.io/porthole/brain/findings/wlroots-drm-layers-need-libliftoff/)

Rejected/failed:
- [chromiumos/platform2 commit d983b6a](https://chromium.googlesource.com/chromiumos/platform2/+/d983b6a2a034737d3e6389365fb1cd42efbe8170) — the fetch returned the repository README, not the commit, so the removal date stays unverified

## Next steps

1. On the RG35XX Pro (available), record `v4l2-ctl --all -d /dev/video0`, `media-ctl -p` and `drm_info`. Do the RG353M later, when it is allowed.
2. Spike route B on the RG35XX Pro against the existing 640x480 fixture. Pass condition: NV12 byte-identical to SHA `0c891fbb…`, then a Fluster JVT-AVC subset.
3. Record the handheld baselines (step G) with the decode-path record. Add the byte-exact decode check beside the cadence measure.
4. Before any HEVC work, track VDPU346 (RK3566 HEVC) and the H616 IOMMU and display fault upstream.

## Suggested issues (for the parent to file)

Use existing labels only (AGENTS.md); add the RG35XX device label only if one exists.

- "Capture V4L2 and KMS facts (v4l2-ctl, media-ctl, drm_info) on the RG35XX Pro for the Rust decoder". Labels: `enhancement`, `priority: medium`, `area: streaming`, `needs-hardware`.
- "Step I spike: Rust H.264 stateless decoder (v4l2r + vendored cros-codecs parser) against the H700 fixture". Labels: `enhancement`, `priority: medium`, `area: streaming`, `needs-hardware`.
- "Plan the Linux 7.4 H616 VE device tree (IOMMU on) against Korri patch 0236 (no IOMMU)". Labels: `maintenance`, `priority: medium`, `upstream`.
- "Correct #391's evidence about cros-codecs (H.264 V4L2 backend exists; not for Cedrus; no HEVC)". Labels: `documentation`, `priority: low`.
