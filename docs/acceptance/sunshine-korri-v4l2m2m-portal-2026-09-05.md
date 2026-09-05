# Portal V4L2 M2M encoding — 2026-09-05

The hardware encoding path passes the four codec/resolution probes. Real Moonlight sessions rendered the Portal console with both codecs at both resolutions. The device is **not production-ready for streaming**: startup packet loss remains, and moving-content throughput was not measured.

## Scope and safety

The test used an Odin 2 Portal running SD-card NixOS, Linux `7.2.0`, and `/dev/video1` (`iris_driver`, `Iris Encoder`). Sunshine captured the physical panel through KMS. Moonlight Embedded `2.7.0` ran on a separate x86-64 device, with SDL software decoding into Xvfb. Hardware **encoding** was mandatory; client software decoding was intentional.

The test loaded a compatible `qcom-iris.ko` temporarily. It checked module version magic and refused to unload a busy encoder. No reboot, boot-file replacement, Android partition write, firmware write, or bootloader operation was used. Sunshine ran as root with an isolated configuration and pairing state under `/tmp`, not as the deployed `korri-linux-host` service. Module and service integration therefore have separate acceptance requirements.

After testing, Sunshine stopped, the original Iris module was restored, and Wi-Fi TX checksum offload was restored. No persistent network change was made.

## Changes and provenance

| Component | Change | Origin |
| --- | --- | --- |
| Sunshine | Register strict `v4l2m2m`, H.264/HEVC, host-memory NV12, and repeated headers. | Korri patch `0021`, reviewed Sunshine `2025.924.154138`. |
| FFmpeg | Copy visible rows using negotiated plane offsets and aligned storage dimensions. | Backport of commit `3fda94e1309bead4d39ea4b2cc42d13f8cdf48b4`, PR `#24328`, onto `61c50407fd429a5e2ec616e2e846c3fe3743879a`. |
| FFmpeg | Add opt-in `repeat_headers=1`, with header-control readback checks. | Korri-local patch `0002`; no upstream commit is claimed. |
| Iris | Implement `V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME`. | Upstream Linux commit `6f62dcefd2494aa9ac01538372353bf07755491e`. |
| Iris | Subscribe to encoder output `HFI_PROP_PICTURE_TYPE`. | Korri-local patch; uses the existing picture-type response handler. |

The [FFmpeg mailing-list archive](https://www.mail-archive.com/ffmpeg-devel@ffmpeg.org/msg190438.html) preserves the alignment fix. The historical `build-deps` revision is `2851db101eeddae8f02489d48a52a4d83e6f7e7b`. It builds the reviewed FFmpeg/CBS libraries rather than upgrading Sunshine's codec ABI.

Approval records and exact patch hashes live in `services/sunshine/approved-patches.nix` and `nix/odin2portal/kernel/default.nix`. The separate `sunshine-korri-v4l2m2m` package does not replace the standard aarch64 software package or the RG353M RKMPP path.

The physical Sunshine run used `/nix/store/3rdsrfw9521z3hd2gxx4xav1bliyj1fr-sunshine-korri-2025.924.154138-korri`. Later changes corrected package provenance and removed one trailing space in the FFmpeg option declaration; they did not change encoder behavior. This report identifies the artifact actually run, rather than presenting a later build as the same physical test.

## Hardware probes

`test-v4l2m2m-encode.c` encodes 120 NV12 frames with a 60 fps time base, a 10 Mbps bitrate request, no B-frames, and GOP 1000. It requests IDRs at frames 0, 30, 60, and 90. This is a finite correctness probe, not a real-time performance benchmark.

`verify-v4l2m2m-streams.py` checks packet count, timestamps, keyframe flags, actual Annex B NAL types, full-stream decode count, and standalone decoding of every IDR packet. It rejects separate header-only packets and recovery IDRs without their own parameter sets.

| Codec | Resolution | Decoded frames | Flagged IDRs | Each IDR decodes alone |
| --- | --- | ---: | ---: | --- |
| H.264 | 1280×720 | 120 | 4 | Yes. |
| H.264 | 1920×1080 | 120 | 4 | Yes. |
| HEVC | 1280×720 | 120 | 4 | Yes. |
| HEVC | 1920×1080 | 120 | 4 | Yes. |

The final rerun used `/nix/store/lirybnlv9r1fh3vyds1n00wi3ibqichw-sunshine-v4l2m2m-probe-1/bin/sunshine-v4l2m2m-probe`, built from the final FFmpeg patch set. All four outputs passed the verifier again. The verifier also rejected the preserved baseline and force-key-only outputs.

HEVC at 1080p emits two slices per frame. Eight type-19 NAL units represent four IDR frames, not eight frames. All four streams reached EOF and decoded without errors. Probe logs are in [the evidence directory](v4l2m2m-portal-2026-09-05/).

The intermediate failures establish why all four dependency fixes matter:

1. Before the alignment fix, standard-height frames crashed in `ff_v4l2_buffer_avframe_to_buf()`.
2. With alignment fixed but the original kernel, all four streams decoded, but forced-key requests returned `Invalid argument`.
3. With force-key support alone, the bitstreams contained four IDRs, but every packet's keyframe flag was zero.
4. With picture-type subscription, four packet keyframe flags appeared. Repeated headers then made every flagged IDR independently decodable, without a separate header packet.

### Reproduce the finite probe

Build `packages.aarch64-linux.sunshine-v4l2m2m-probe` on an aarch64 builder. The probe links to the same static FFmpeg recipe as the V4L2 Sunshine package. On the Portal, with the patched Iris kernel/module already active, run:

```sh
nix-shell services/sunshine/run-v4l2m2m-probe.sh /path/to/sunshine-v4l2m2m-probe
```

The runner prints a fresh output directory. Copy that directory to the verification machine, then run:

```sh
nix-shell services/sunshine/verify-v4l2m2m-streams.py /path/to/output-directory
```

These scripts do not install a kernel, load modules, or change boot files. A device kernel cutover remains a separate operation.

## Sunshine and Moonlight

Sunshine's own validation reported `PASSED`, `VUI_PARAMETERS: supported`, and these selected encoders:

```text
Found H.264 encoder: h264_v4l2m2m [v4l2m2m]
Found HEVC encoder: hevc_v4l2m2m [v4l2m2m]
```

HDR, 4:4:4, and AV1 were unsupported. Expected probes of unsupported formats logged errors; H.264 and HEVC SDR validation succeeded. `SUNSHINE_STRICT_ENCODER=1` remained set throughout.

The first network tests did not render a usable stream. For example, Moonlight reported `Unrecoverable frame 1: 5+0=5 received < 48 needed` for HEVC 720p. The server emitted real IDRs and repeated parameter sets on the wire, but the client waited for recovery.

With TX checksum offload temporarily disabled on `wlp1s0`, all four 20-second sessions rendered the physical console. Each session negotiated 60 fps and 10 Mbps; those are requested settings, not measured sustained throughput. Each client exited cleanly after SIGINT and requested that Sunshine close the application.

| Session | Rendered screenshot | Startup observations |
| --- | --- | --- |
| H.264 720p | [Screenshot](v4l2m2m-portal-2026-09-05/h264-720p.png) | One network-drop message; 19 waiting-for-IDR messages. |
| H.264 1080p | [Screenshot](v4l2m2m-portal-2026-09-05/h264-1080p.png) | 121 waiting-for-IDR messages. |
| HEVC 720p | [Screenshot](v4l2m2m-portal-2026-09-05/h265-720p.png) | One network-drop message; 22 waiting-for-IDR messages. |
| HEVC 1080p | [Screenshot](v4l2m2m-portal-2026-09-05/h265-1080p.png) | 121 waiting-for-IDR messages. |

The screenshots and matching client logs are preserved together. The panel console is rotated; this test did not install a compositor or solve display orientation. Audio and remote input were disabled. The client log's failed attempt to open `h264_v4l2m2m` or `hevc_v4l2m2m` is its **decoder** selection, followed by the intended SDL software decoder. It is not a Sunshine encoder fallback.

**Inference, not proof:** disabling checksum offload may reduce this packet loss. Startup loss remained, and no controlled on/off/on/off comparison was completed. Do not turn this diagnostic into persistent network policy without that evidence.

## Automated gates

The final targeted builds passed on x86-64 and natively on aarch64 (`fuji`).

| Check | x86-64 | aarch64 |
| --- | --- | --- |
| `sunshine-korri-v4l2m2m` | Passed. | Passed, including the V4L2 Sunshine binary. |
| `korri-linux-host-module` | Passed. | Passed. |
| `sunshine-korri-package` | Passed. | Passed, including the standard software profile. |
| `sunshine-korri-runtime-settings` | Passed. | Passed. |
| `sunshine-korri-input-seat-patch` | Passed. | Passed. |
| `sunshine-korri-certificate-control` | Passed. | Passed. |
| `sunshine-v4l2m2m-probe` package | Not offered. | Built and physically run. |

The Portal kernel and module cross-build also passed. Nix formatting and whitespace checks on non-patch files passed. Unified-diff context lines in carried patches retain their required leading spaces.

The full `nix flake check --no-build` did **not** pass. It failed at `korrid-linux-device-module` because `/nix/store/jz7q5l6a7klv36wjd1gsp526l7bg01gj-korrid-validate-owner-binding.drv` was not valid. The same failure reproduced with evaluation caching disabled on unchanged base commit `128bb405cc3c3b28ea660245f06618c08e9402d4`. Backlog item `01M1S7C10B0PG8WAR7MV7T56PZ` records that separate failure.

## Remaining release gates

- Diagnose Wi-Fi loss with captures at both ends and a controlled offload comparison. Backlog item `01M1S6KJXFBCQQ11SXT2BSBAQ6` records this work.
- Verify sustained moving-content 720p60 and 1080p60, recovery after loss, and repeated reconnects with both codecs.
- Verify the deployed service through `services/inputd/nix/korri-linux-host.nix`, including its permissions, capture session, audio, and input. Root-level KMS smoke tests do not prove the service sandbox.
- Keep HDR, AV1, 4:4:4, and live bitrate changes unavailable for this profile. Keep strict selection; do not hide hardware failures with software fallback.
