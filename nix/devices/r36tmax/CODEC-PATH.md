# Video codec path on Linux 7.2.6

H.264 hardware decoding passed one bounded test on the R36T Max on
2026-09-15. Hantro decoded eight 320×240 frames. All output bytes matched a
software decode of the same compressed file on the build host.

This result does not establish browser acceleration, streaming performance,
other codecs or sustained operation. No kernel or device configuration changed.

## Hardware result

The test used stock GStreamer 1.26.5 with an explicit `v4l2slh264dec` element.
The media topology paired `/dev/media0` with the Hantro decoder at `/dev/video1`.
The driver reported `rockchip,px30-vpu-dec` and version `7.2.6`.

| Gate | Observed result |
| --- | --- |
| Input | Eight H.264 Constrained Baseline frames, 320×240, 8-bit 4:2:0, at 5 frames/s. |
| Request interface | Eight successful `MEDIA_REQUEST_IOC_QUEUE` calls used three reusable request descriptors. |
| Request controls | Eight successful request-scoped `VIDIOC_S_EXT_CTRLS` calls preceded the compressed buffers. |
| Capture | Eight successful capture dequeues returned timestamps in order, with no error flags. |
| Output | 921,600 bytes of I420 matched the host reference through both SHA-256 and `cmp`. |
| Pipeline | GStreamer reached EOS and exited with status 0. |
| Interrupts | The decoder counter increased from 0 to 8. Encoder and VPU IOMMU counters stayed at 0. |
| Kernel | Before and after kernel logs were identical. |
| Temperature | The test started at 55.909 C. The sampled maximum was 62.5 C. |
| Recovery state | The browser and compositor were active after restoration. SSH and bounded file transfers worked. |

The tested chain was:

```text
filesrc ! h264parse ! video/x-h264,stream-format=byte-stream,alignment=au !
v4l2slh264dec media-device=/dev/media0 video-device=/dev/video1 !
video/x-raw,format=NV12,width=320,height=240 ! videoconvert n-threads=1 !
video/x-raw,format=I420,width=320,height=240 ! filesink
```

No software decoder or automatic decoder selection appeared in this chain.
`h264parse` parsed the compressed stream. `videoconvert` converted decoded NV12
to planar I420 for byte comparison. The host reference came from the same H.264
file, not from the original uncompressed test pattern.

The output and reference SHA-256 was:

```text
0c6d7c6d3a1775152fb7b2595bdbf7eb072c7ef74cc528bc33b0177d2580308e
```

The compressed input SHA-256 was:

```text
1dd76d2e4bc439ae6ed6c371d8bb3992f463908c55df8911fd474a7318394ef0
```

## Test conditions and delivery

The owner approved a browser pause and one bounded hardware test. The test
required three temperature samples below 60 C before submission. The supervisor
stops the worker if the temperature reaches 70 C or a new kernel warning appears.
Independent systemd and userspace limits bound execution to 20 seconds, with a
further kill grace period of two seconds. No abort limit triggered during this
decode. These limits cannot recover every kernel hang.

The existing image already contained the GStreamer core, base and bad-plugin
libraries. Signed prebuilt tools added 27 store paths from `cache.nixos.org`.
The download was 41.32 MiB, with 308.37 MiB unpacked. Builds remained disabled,
`fallback` remained false, and signature checks remained required.

| Component | Exact output |
| --- | --- |
| GStreamer commands | `/nix/store/2jr2bjms36x377dmsy5h0bi7sgm89ck8-gstreamer-1.26.5-bin` |
| GStreamer core | `/nix/store/wmp2mqc7svy5s6pq0pjpn8q0d58kgdlq-gstreamer-1.26.5` |
| Base plugins | `/nix/store/zcwzcchfdc5wc164xz91wx5w4w884225-gst-plugins-base-1.26.5` |
| Bad plugins, including `v4l2codecs` | `/nix/store/xng741n6pnj4sis4jyx7xj4jrhp2jfc5-gst-plugins-bad-1.26.5` |
| Topology and format tools | `/nix/store/8lfxrndhs7y6qm75ii55zyqkjc7inj1r-v4l-utils-1.32.0` |

This was diagnostic delivery, not an image or profile update. The added tools
have no new permanent GC root. Their presence is not a browser integration.

The first launcher failed before GStreamer started. Its transient systemd
service could not find `cat` or `sha256sum` and exited with status 127.
An explicit `PATH=/run/current-system/sw/bin` fixed that launcher. A service
smoke test passed before the owner-approved corrected decode attempt.

Large evidence transfers also coincided with loss of USB network and serial
access, including after a recovery boot without a codec run. The cause remains
unknown. After USB reconnection, selected transfers at 256 kbit/s completed.
This does not establish that rate limiting fixes the USB defect.

## Driver capabilities and remaining limits

The Linux 7.2.6 `px30_vpu_variant` declares:

```c
.codec = HANTRO_JPEG_ENCODER | HANTRO_MPEG2_DECODER |
         HANTRO_VP8_DECODER | HANTRO_H264_DECODER,
```

| Direction | Advertised formats | Acceptance |
| --- | --- | --- |
| Decode | H.264, VP8, MPEG-2 | Only the H.264 fixture described here passed. |
| Encode | JPEG only | No successful encoder test. |

`px30.dtsi` supplies `video-codec@ff442000`, its VPU power domain and its IOMMU.
`dts/config` enables Hantro and the stateless codec helpers. The inherited
`CONFIG_VIDEO_ROCKCHIP_VDEC=m` does not add another decoder without a matching
PX30 device-tree node.

Mainline exposes no H.264 encoder for this variant. Configuration changes do
not add one. Vendor MPP/VEPU2 work remains quarantined. Read
`.pi-web/handoffs/a3299ec1-9f0d-47a6-96c0-49d08ab29fba.md` before that separate work.

The earlier JPEG test caused an IOMMU fault and disabled its interrupt. The
source-visible encoder format-state defect remains in Linux 7.2.6. This decode
test did not exercise that encoder branch or repair it. Do not repeat the
faulting JPEG command or bypass IOMMU protection. Stage 4 of
[the hardware survey](HARDWARE-SURVEY-2026-09-14.md#stage-4-first-codec-job-failed)
records the failure.

No Chromium configuration changed. Other profiles, resolutions, codecs,
zero-copy display, game streaming and sustained performance need separate
acceptance. The traced eight-frame test is not a throughput benchmark.

## Evidence

The accepted boot ID was `f10e3193-bc70-491e-b124-a53c2ee97d6a`. The active system
was `/nix/store/x841s8hhifiqy8v3i6xz30p3xlgxm12b-nixos-system-r36tmax-sd-card-26.05.20251221.a653104`.

Private host evidence is under:

```text
~/.local/share/korri/rk3326-stock-shell/logs/hardware-usb-r36tmax-h264-20260915-163414/
```

The directory contains the fixture, software reference, returned pixels,
reassembled-call verifier, raw ioctl trace, topology, plugin identity, thermal
samples, kernel logs, launcher failure and executed scripts. `verification.txt`
records the host-side result. The retained verifier runs without device access:

```sh
python3 "$evidence/verify.py" "$evidence"
```

Set `evidence` to that directory first. The archived device scripts contain
exact paths for this test. They are evidence, not unattended replay commands.
Any new codec job requires current boot, topology, temperature and recovery
checks with owner readiness. Do not reload RK915 as part of codec testing.
