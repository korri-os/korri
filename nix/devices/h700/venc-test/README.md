# H700 stateful encoder acceptance

Patch `0237-h616-ve-h264-encoder.patch` adds `/dev/video1` to
`sunxi-cedrus`. `/dev/video0` remains the stateless decoder. Both nodes use
one m2m scheduler, IRQ, runtime-PM owner, reset, SRAM claim and `ve-pool`.
There is no second platform device and no IOMMU attachment.

## Contract

| Consumer requirement | Implementation |
|---|---|
| FFmpeg `61c50407fd429a5e2ec616e2e846c3fe3743879a` | Single-planar `VIDEO_OUTPUT` NV12 and `VIDEO_CAPTURE` Annex-B H.264; MMAP and contiguous DMABUF queues |
| Aligned buffer layout | Width/height rounded to 16; NV12 stride and `sizeimage` returned by S_FMT; crop selection produces SPS cropping; reconstruction rows aligned to 128 |
| Sunshine `repeat_headers=1` | `HEADER_MODE_JOINED_WITH_1ST_FRAME` reads back; `PREPEND_SPSPPS_TO_IDR=1` reads back; SPS/PPS and frame share one capture buffer |
| Profile/level | Main and High, CABAC, 8-bit 4:2:0; levels 4.1 and 4.2; profile/level changes require stream restart |
| GOP and B frames | Intra-only fallback: GOP range exactly 1..1; B-frame range exactly 0..0; every capture frame carries KEYFRAME |
| Client IDR requests | FORCE_KEY_FRAME accepts the standard button operation; every frame is already an IDR |
| Bitrate | Frame-level QP feedback driven by BITRATE and OUTPUT timeperframe; FRAME_RC_ENABLE=0 uses I_FRAME_QP |
| Drain | ENC_CMD_STOP, LAST, EOS subscription/event, empty drain with a subsequently queued capture buffer, ENC_CMD_START |
| Completion/recovery | Frame completion uses the AVC interrupt; shared 500 ms watchdog resets the VE and returns failed buffers; no frame-completion polling |

Separate sequence-header packets and Baseline/CAVLC are **not** advertised.
The Sunshine profile requests joined headers. FFmpeg's default separate-header
request is optional and it can still use the driver's joined output. This
is not a claim of support for every stateful encoder control or every FFmpeg
application. Timeout reset code is implemented but not fault-injected on the
handheld. DMABUF import is not a tested zero-copy GPU capture path.

## P-frame result

Two experimental builds adapted the T527 P path: `0xb04=0x510` (0x410 plus
CABAC), `0xb2c=0xc000c852`, ping-pong reference/reconstruction planes,
128-row reconstruction alignment, P-slice headers, and reference addresses.
The first kept H700 `0xb14=0xf`; the second used T527's P value `0x12040007`.
Both completed 120 frames without a kernel fault, but FFmpeg reported invalid
macroblock modes and corrupt P pictures. Neither is shipped. The patch keeps
only the trace-proven IDR path and aliases reference/reconstruction as the
physical replay did. A real H700 multi-frame vendor trace remains necessary.

## Rate control and register evidence

The H700 trace fixes these values. Their bit fields are not established:

| Register | Shipped value | Measured interpretation |
|---|---|---|
| 0xb2c | 0x80001900 | Traced IDR setup; no independent field sweep |
| 0xb30 | 0 | Traced QP table word; no independent field sweep |
| 0xb34 | 0x261e1814 | Traced QP table word; no independent field sweep |
| 0xb38 | 0xffff3630 | Traced QP table word; no independent field sweep |
| 0xb40 | 0x74000000 | Traced RC word; no independent field sweep |
| 0xb44 | 0 | Traced setup word; no independent field sweep |
| 0xb48 | 0x00180004 | Traced RC word; no independent field sweep |
| 0xb4c | 0x00800068 | Traced RC word; no independent field sweep |
| 0xb08 | 0x08000500 OR QP | Global QP, also written into the slice's QP delta; measured at QP 18 and 36 |

Do not label unknown fields as a hardware CBR implementation. The software
feedback loop changes only QP, keeps a bounded signed bit-budget error, and
limits QP changes to one per frame. This satisfies bitrate-dependent output
size, not tight CBR, HRD or filler-NAL guarantees. The CPU writes headers with
the hardware bit writer; it does not encode macroblocks.

The 640x480 Main level-4.1 headers match the traced 22 bytes exactly:
`00000001674d002996540501e8800000000168ee3c80`.

## Reproduce off-device

Build the Pro system and guard:

```sh
nix build --no-update-lock-file -L --out-link /tmp/h700-venc/system \
  .#nixosConfigurations.rg35xxpro.config.system.build.toplevel
nix build --no-update-lock-file -L --out-link /tmp/h700-venc/check .#checks.x86_64-linux.h700
```

Build `korri-plugins`' unmodified `packages.aarch64-linux.sunshine-v4l2m2m-probe`.
`ffmpeg-probe.nix` takes that derivation as `probe` and changes only the test
source. It retains the approved static FFmpeg and its alignment/header patches.
The extended source defaults to 600 frames. `FRAMES`, `SECONDS` (minimum run
duration) and `BITRATE` can change the workload. It reports wall time, fps and
process user+system CPU time. `controls-probe.nix` takes an off-device aarch64
`stdenv`; it tests fixed QP, profile/level readback, alignment/cropping, forced
IDR requests, LAST/EOS and restart.

Sign and copy prebuilt closures with the existing pinned Pro transport. Compare
the candidate `kernel` byte-for-byte with the booted system before loading any
module. Check the decoder's module reference count is zero. Temporarily replace
only `sunxi-cedrus.ko`, test, then unload it and `modprobe sunxi-cedrus` from the
original running system. Never select the candidate system profile, change boot
entries, reboot, or write a card partition or firmware. On a hard hang, stop.

Host checks:

```sh
nix-shell -p python3 python3Packages.numpy ffmpeg --run \
  'cd nix/devices/h700/venc-test && python3 -m unittest -v test_verify.py'
nix-shell -p python3 python3Packages.numpy ffmpeg --run \
  'python3 nix/devices/h700/venc-test/verify.py STREAM.h264 STREAM.log \
    --sunshine-verifier /path/to/korri-plugins/services/sunshine/verify-v4l2m2m-streams.py'
```

The verifier imports the actual Sunshine verifier's `decode` and `nal_types`
functions. Its matrix differs deliberately: 640x480, 600 frames, every frame
IDR, rather than four 720p/1080p H.264/HEVC streams with P frames. It checks every
packet's IDR/SPS/PPS, independently decodes the 20 requested recovery frames
and the final frame, checks drain/PTS, and measures every Y plane against the
moving source. It does not substitute a software stream for hardware evidence.

Raw build/device/host evidence and the worksheet section-10 draft are under
`/tmp/h700-venc/` on the development host. This directory is not a deployment
artifact. The Sunshine service and an end-to-end Moonlight session are not
installed or tested by this kernel slice.
