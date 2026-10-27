# H700 Moonlight hardware decode through V4L2 Request

## Scope

The RG35XX Pro and RG35XX SP decoded Moonlight streams in software. Their
H700 has Cedrus, a stateless decoder, so `h264_v4l2m2m` cannot open it.
Cedrus has worked in the kernel since `9defda451`
(`docs/research/h700-video-engine-worksheet.md`, section 6). This change
selects Moonlight's `v4l2request` platform on the H700. It also makes that
platform fast enough to use there.

## Device facts

Read on 2026-10-27 on Linux `7.2.0`:

| Fact | RG35XX Pro | RG35XX SP |
| --- | --- | --- |
| Device-tree compatible | `anbernic,rg35xx-pro`, `allwinner,sun50i-h700` | `anbernic,rg35xx-sp`, `allwinner,sun50i-h700` |
| Decoder node | `/dev/video0`, `cedrus` | `/dev/video0`, `cedrus` |
| Encoder node | `/dev/video1`, `cedrus-encoder` | `/dev/video1`, `cedrus-encoder` |
| Capture format chosen by FFmpeg | `NV12` (linear) | `NV12` (linear) |
| GPU | Mali-G31 MC1, Panfrost, `EGL_EXT_image_dma_buf_import` | same |

## Change

- `korri-os/plugins` patch `0022-v4l2request-zero-copy-gl-background-window.patch`
  (source `dbb0917`, batch `build-dbb0917a5b58-moonlight-h700-gl`) builds on
  patch 0021 from the RG353M work:
  - Each Cedrus capture buffer is imported once into the SDL window's GLES
    context as R8 (Y) and GR88 (UV) EGL images, then drawn with the NV12
    shader. No frame is copied by the CPU. The last frame shown stays
    referenced until the next one is drawn.
  - The window and GLES context are created on the display thread, and
    `setup()` returns at once. On the H700, SDL window and EGL bring-up took
    9 to 12 s, which is longer than Sunshine waits for the first ping. Patch
    0021 alone ended with `Initial Ping Timeout` and `No video traffic was ever
    received from the host!`.
  - The window is fullscreen. Without that, sway split the screen between the
    viewer and the portal, at 320x480 each.
  - The YUV matrix and range come from the stream when it states them.
    Otherwise they come from the colorspace in each decode unit, with limited
    range. The SP's Cedrus encoder writes no VUI.
- korrid `services/korrid/src/linux_viewer/production.rs` selects
  `v4l2request` for device-tree `allwinner,sun50i-h700`.

## Verification

### Decoder correctness

The patched FFmpeg (`-hwaccel v4l2request`, `-hwaccel_output_format
drm_prime` and an NV12 download) decoded two libx264 clips. Each clip is
640x480 at 60 fps and has 120 frames. The output was byte-identical to a host
software decode.

| Device | Clip | Result |
| --- | --- | --- |
| Pro | High profile, CABAC | match |
| Pro | Constrained Baseline | match |
| SP | High profile, CABAC | match, while Sunshine encoded on the same VE |

### Live stream, Pro joins SP

Turtles in Time ran on the SP at 640x480. The Pro joined through an invite.
The new Moonlight binary was bind-mounted over the installed one, and korrid
passed `-platform v4l2request`. Each measurement started after the viewer
window appeared and lasted 6 s.

| Run | Decode | Moonlight CPU (one core) | Pro panel flips/s | SP panel flips/s |
| --- | --- | --- | --- | --- |
| `hw11-1` | hardware | below 2.5 % (absent from top 6) | 45.7 | 44.0 |
| `ab-hw-1` | hardware | 20.7 % | 37.2 | 41.5 |
| `ab-hw-3` | hardware | 30.5 % | 19.0 | 48.0 |
| 13:30Z | hardware, published source `dbb0917` | 26.8 % | 38.8 | 42.7 |
| `ab-sw-3` | software (`x11`) | 97.2 % | 53.7 | 51.2 |

The screenshot `.forgerie/stream-play/shots/stream-hw11-1.png` shows the game
fullscreen on the Pro, with correct colour. In `ab-hw-3`, the Pro's korrid
used 199 % CPU; see #289. Software decode presents through SDL's renderer,
so its flip count is not a decoded-frame count.

### Production stream play

Both handhelds ran system `0fba3f684` (SP
`jxcgxaw5sa4s3rxbv28p2m8pnvk9v7r3`, Pro `vaniwvjbdq68yqbyfps52q0rmwbrnp2v`,
bundle `19cgkv6dwzw9s9mz7vzlcwdgknjgabvv`). `@korri:moonlight` was updated on
each from the Pages cache to
`/nix/store/84cqgpjixgcf1a96g6gpgqvvpr475xfg-korri-plugin`, by exact package
path and approval digest. korrid launched Moonlight with
`-platform v4l2request`, and Moonlight logged
`decoder=v4l2request presenter=prime-gl`. Each measurement lasted 6 s.

| Direction | Game | Moonlight CPU (one core) | Joiner panel flips/s | Holder panel flips/s | Joiner VE interrupts |
| --- | --- | --- | --- | --- | --- |
| Pro joins SP (`prod-pro-1`) | Turtles in Time | 22.3 % | 33.2 | 41.0 | 345 |
| SP joins Pro (`rev4`) | Wario Land 4 | 30.2 % | 53.2 | 49.3 | 532 |

Both viewers were fullscreen at 640x480. The SP screenshot is
`.forgerie/stream-play/shots/reverse-rev4.png`.

## Limits

- A Pro viewer that died in an earlier join left the Pro unable to start games
  until its host-session record was removed by hand (#302).
- Stream plays still end after 20 to 70 s, when one holder status read times
  out (#288). Every run above ended that way.
- Patch 0022 also changes the RG353M's `v4l2request` presentation from the CPU
  map to the zero-copy GL import. The RG353M (`haku`) was not available for
  this test.
- One FFmpeg decode test on the SP ran at about 07:00Z, while Sunshine was
  encoding. A different system had been installed on the SP at 06:01Z. By
  about 07:10Z the SP no longer answered SSH, and it needed a power cycle.
  The cause is not known.
