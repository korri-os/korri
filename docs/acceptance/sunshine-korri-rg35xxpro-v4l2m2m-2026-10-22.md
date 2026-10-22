# RG35XX Pro Sunshine / Cedrus H.264 — acceptance, 2026-10-22

**640×480 at sustained 60 fps was not achieved.** Hardware H.264 streaming works end-to-end, including correct decoded Pico pictures. Sustained received rates were **50.567 fps over Wi-Fi** and **50.410 fps over USB Ethernet**. USB eliminated the observed Wi-Fi frame losses but did not improve throughput. The evidence points to CPU capture/readback/conversion, not the VE encoder, as the throughput limit.

## Scope and safety

Test window: 08:50–09:22 UTC. Host: zao. Device: Anbernic RG35XX Pro, 1 GiB RAM, four Cortex-A53 cores, 640×480 DSI-1 at 60 Hz, Sway and Pico Chromium kiosk. Wi-Fi driver verified as `/sys/bus/sdio/drivers/rtw88_8821cs` (RTL8821CS).

No reboot, system-profile selection, boot-entry change, card-partition write, firmware write, on-device compilation, push, or land occurred. All builds ran on zao. Closures were signed with the existing cache key and copied through pinned SSH. Plugin approval, signature verification, device permissions, and Moonlight authentication stayed enabled. No network settings were changed.

The original device module was restored after testing. The updated Sunshine plugin remains installed but Disabled. The fix is committed only in branch `fix/h700-venc-pool-budget`, worktree `/home/simonwjackson/code/sandbox/korri/.worktree/h700-venc-mem`, commit **`d879f951afa3006113600999ec5cebf350cd9a4c`**. Parent authorization expanded this task to that driver fix after the published package's startup probe exposed its pool over-allocation. The publisher repository was read-only throughout.

## Exact artifacts

| Component | Artifact actually used |
|---|---|
| Running system and unchanged boot default | `/nix/store/xqxwcwjqw6grf0wzz4aq7ydai7da83bx-nixos-system-rg35xxpro-sd-card-26.11.20260923.4975466` |
| Running kernel Image SHA-256 | `45ae382a9c700da5623a99d6a43d0d9644a8a6e51a7a77d0b30518d94b885756` |
| Previous installed plugin | `/nix/store/33w3ig2ilv4fhblc4gi4aa72l4y8xh06-korri-plugin` |
| Updated published plugin | `/nix/store/7q3fv7767vdzrbax2v2dz2chcvp2yf7r-korri-plugin` |
| Sunshine executable | `/nix/store/v9v77hxz3h0syqpi2vk0pqm75x4437qs-sunshine-korri-2025.924.154138-korri/bin/sunshine` |
| Native runtime | `/nix/store/67d4jnlqfd6qix3d24pphaj2k8f2kmqz-korri-sunshine-run/bin/korri-sunshine-run` |
| Build profile | `aarch64-linux-rkmpp-v4l2m2m` |
| Tested compatible fixed module | `/nix/store/k6v3faxl39gsdrbyfsq9wm971nhrja2b-h700-venc-pool-budget-module-aarch64-unknown-linux-gnu/lib/modules/7.2.0/sunxi-cedrus.ko` |
| Fixed module's running-kernel development files | `/nix/store/0wj571ay4j5q68r02qcdlq0l6g66ckhx-linux-aarch64-unknown-linux-gnu-7.2-dev` |
| Client | `/nix/store/388lgllfhliykj646jmymximdn83a18a-moonlight-embedded-2.7.0/bin/moonlight` |
| Matching client source for observer ABI | `/nix/store/qh2z30p0mwp6ijzihraj52qqa9wjw25k-source` |

Sunshine's unchanged provenance is in `sunshine-provenance.txt`. The software encoder was not used for the accepted sessions. The successful startup and real sessions log:

```text
Creating encoder [h264_v4l2m2m]
Using device /dev/video1
 driver 'cedrus-encoder' on card 'cedrus-encoder' in splane mode
Found H.264 encoder: h264_v4l2m2m [v4l2m2m]
```

`/dev/video0` remained the stateless decoder. `encoder=auto` was kept. No encoder forcing was necessary after the driver fix.

## Initial failure and driver fix

The first startup could not capture because DSI-1 was idle-powered-off. Waking the existing output through `swaymsg ... output DSI-1 power on` fixed that prerequisite without restarting Pico, korrid, or Sway.

With the panel awake, auto tried NVENC and VAAPI, then reached the correct `h264_v4l2m2m` node. The encoder failed at STREAMON with `Cannot allocate memory`. Sunshine probes **1920×1080**, irrespective of the later client request. Temporary driver diagnostics measured FFmpeg's 16 OUTPUT / 4 CAPTURE requests and the actual negotiated 1920×1088 storage geometry. The first internal reconstruction allocation failed; it was not a failure of the native-size 640×480 encode operation.

### Per-context budget at the startup probe

All sizes below are page-aligned bytes. The three conservative work-buffer requirements remain undocumented; they were not reduced to guessed sizes. The CAPTURE size was already resolution-derived, not a fixed multi-MiB allocation.

| Allocation | Old | Fixed |
|---|---:|---:|
| OUTPUT NV12, 3,133,440 B each | 16 × = 50,135,040 | 8 × = 25,067,520 |
| CAPTURE H.264, 3,133,440 B each | 4 × = 12,533,760 | 4 × = 12,533,760 |
| Reference/reconstruction (aliased intra-only) | 3,440,640 | 3,440,640 |
| Small-luma | 552,960 | 552,960 |
| MB information | 3,440,640 | 3,440,640 |
| Scratch | 3,440,640 | 3,440,640 |
| Extra / register 0xb60 | 3,440,640 | 3,440,640 |
| External line buffer, width ≤1920 | 0 | 0 |
| **Total** | **76,984,320** | **51,916,800** |

The old queues alone consumed 62,668,800 B. Reconstruction requested another 840 pages and failed. The fixed total leaves **14.48828125 MiB**, rounded to **14.49 MiB**, in the existing 64 MiB `ve-pool`.

Patch 0237 now caps `queue_setup` at eight OUTPUT and four CAPTURE buffers. FFmpeg accepts the lower returned count. Existing `CREATE_BUFS` allocations count toward the cap. The test requests the real FFmpeg defaults and verifies that an append at the cap fails with ENOBUFS. The cap is per context; it does not reserve the pool globally against other consumers.

Reproduction before the fix: `controls-repro.log` returned 16 OUTPUT buffers and failed the new bounded-count assertion. After the fix the same test returned 8/4 and encoded successfully. Sunshine's unchanged 1080p startup probe then succeeded.

### Source identity

The compatible module was cross-built with `W=1` against the fdd-based running kernel's development files, not against the changed current-main Image. All compiled Cedrus C/H files match the committed patch applied to the pristine fdd-based Cedrus source (`patch-applied/`, `patch-apply.log`, `patch-identity.log`). A kernel tree archived from fdd-based commit `4211683ce5d50ecd69a1ba13f7c152d18d7bdd93` is retained at `fdd-source/` with the updated patch. `cmp` confirms that patch and the main-based worktree patch are byte-identical. The actual patch stored in the successfully built main kernel derivation matches too:

```text
/nix/store/i6ghbiiavcl6yk5wcjl9zkf0rky96932-0237-h616-ve-h264-encoder.patch
SHA256 9b3ac6d6d88c1d54a131c6f92cd52e33b120632a02eab61696b82d40f8694d04
```

The newly built main-based Image was **not booted**. The running Image matches the fdd-system Image byte-for-byte by SHA-256.

### Rejected candidate warnings — not omitted

**Correction to the interim report:** the complete kernel journal contains **four WARN traces at two timestamps**, not just the two controls-probe traces first observed.

A rejected candidate set `vb2_queue.max_num_buffers` to 8/4. Linux 7.2 requires that field to be at least `VB2_MAX_FRAME`; opening the node therefore returned EINVAL and hit its guard at `videobuf2-core.c:2614`.

| UTC timestamp | Process |
|---|---|
| 09:01:25 | controls-probe, PID 4607 |
| 09:01:25 | v4l_id, PID 4616 |
| 09:01:25 | wireplumber, PID 987 |
| 09:01:34 | controls-probe, PID 4632 |

Shared stack: `vb2_core_queue_init -> vb2_queue_init[_name] -> cedrus_enc_queue_init -> v4l2_m2m_ctx_init -> cedrus_enc_open -> v4l2_open`, followed by the caller's open syscall path. That rejected module was `/nix/store/xqvlsbzzhgrq98b4pqh7si751fjwygg5-h700-venc-pool-budget-module-aarch64-unknown-linux-gnu`. It was unloaded and was not used for streaming. The committed fix does not set `max_num_buffers`.

The current candidate's window from **09:03 onward** contains no new WARN, Oops, BUG, encoder timeout, IOMMU fault, or IRQ-storm report. Full earlier CMA allocation-failure traces and these four WARNs remain preserved in `kernel-full-and-final-config.log`. Literal zero warnings for the complete test is **not** claimed. Final kernel taint is 5632; the WARN taint cannot be removed without a reboot, and no reboot was permitted.

## Plugin approval and authenticated pairing

Approval was computed on zao using the existing image-seeding producer, not manufactured on the device:

```sh
/nix/store/yqik19gx65f9czjn8lzzg6056dx1606j-korri-plugin-host-0.0.0/bin/korri-plugin seed-graph \
  /nix/store/yq934gf72zc70w8bf2ihbjz0r0lc8m3z-published-plugin-publishers.json \
  /nix/store/7q3fv7767vdzrbax2v2dz2chcvp2yf7r-korri-plugin
```

Approval: `8c54a706f8b04826434f0a420f14db9ccbe036931377d521d65916f911b5eacf`.

On the Pro:

```sh
korri-plugin update @korri:sunshine https://github.com/korri-os/plugins/releases/download/cache/ \
  /nix/store/7q3fv7767vdzrbax2v2dz2chcvp2yf7r-korri-plugin \
  8c54a706f8b04826434f0a420f14db9ccbe036931377d521d65916f911b5eacf
korri-plugin enable @korri:sunshine
```

The host recomputed and accepted the approval and verified the closure. Certificate provisioning through korrid's local portal capability returned HTTP 403: this device's `LocalSessions` permission excludes that RPC. That refusal was respected; no permission was widened and no certificate-control socket was accessed as an unauthorized peer.

Pairing therefore used Sunshine's normal **authenticated PIN endpoint**, over loopback on the Pro. The installation had no admin credentials or `sunshine_state.json`. A temporary administrator was initialized with `/api/password`; `/api/pin` then required its HTTP Basic credentials. `nix shell nixpkgs#moonlight-embedded -c moonlight pair 192.168.1.163 -keydir /tmp/h700-sun/client` completed successfully. Authenticated `moonlight list` returned Desktop, Low Res Desktop, and Steam Big Picture. Only **Desktop**, which has no launch command, was selected. Credentials were not logged in this record. Pairing and test-only admin state were removed from the live configuration during cleanup.

## Sustained streams

The unchanged Moonlight Embedded 2.7.0 fake renderer does not decode or publish fps statistics. `client-metrics.c` is an off-device observation adapter loaded with LD_PRELOAD. It wraps `LiStartConnection` and replaces only the NULL fake video callbacks with callbacks that count complete decode units and return the same `DR_OK`. It does not change transport, FEC, pairing, or server settings. It records frame-number gaps, assembly time, host-provided latency, and RTT. For the picture run it also writes the received Annex-B buffer chain to a file. The source and compiled observer are retained here. A failed first observer launch preloaded it into `stdbuf`, which lacked libcrypto symbols; no stream began. The successful commands load it into `moonlight` only.

```sh
# Wi-Fi: 190 s command timeout; 187.750 s observed video
nix shell nixpkgs#moonlight-embedded -c env LD_PRELOAD=/tmp/h700-sun/client-metrics.so \
  moonlight stream 192.168.1.163 -platform fake -viewonly \
  -width 640 -height 480 -fps 60 -codec h264 -bitrate 10000 \
  -app Desktop -keydir /tmp/h700-sun/client -quitappafter

# USB Ethernet: 195 s timeout; 192.641 s observed video
# Same command, address 10.42.6.1 and -bitrate 20000.
```

`timeout --signal=INT --kill-after=15` stopped each client cleanly. Its status 124 denotes the scheduled timeout, not a streaming crash. USB used the existing direct link: zao `10.42.6.12`, interface `enp90s0u2u3u1`; Pro `10.42.6.1`, `usb0`. Control SSH stayed on pinned Wi-Fi port 2222.

Pico stayed on its idle welcome screen: colored cartridge tiles scroll horizontally; “PRESS ANY BUTTON” appears/blinks. No game was launched and no remote input was injected. Pictures below establish actual motion, rather than a synthetic encoder pattern.

| Measurement | Wi-Fi, 10000 kbps request | USB, 20000 kbps request |
|---|---:|---:|
| Requested mode | 640×480, 60 fps, H.264 | 640×480, 60 fps, H.264 |
| Received complete frames | 9,495 | 9,712 |
| Observed video duration | 187.750 s | 192.641 s |
| Received fps, `(frames−1)/time` | **50.567** | **50.410** |
| Network-dropped frames | 5, each logged unrecoverable | 0 |
| Decoder drops/errors in fake run | Not applicable: no decoder | Not applicable: no decoder |
| Received coded bytes | 151,167,080 | 169,665,664 |
| Mean frame assembly time | 1.422 ms | 0.371 ms |
| Mean host-provided processing latency | 92.034 ms | 92.021 ms |
| Mean receiver queue delay | 0.032 ms | 0.020 ms |
| Final RTT / variance sample | 7 / 11 ms | 15 / 25 ms |
| Ping packets | 195/195, 0% loss | 195/195, 0% loss |
| Ping RTT min/avg/max | 1.606/5.656/268.118 ms | 1.701/1.923/3.150 ms |
| Sunshine logged video bitrate | 7,308,000 bps | 14,988,000 bps |
| Steady VE IRQ rate | 50.546/s | 50.388/s |
| Sunshine CPU, one core = 100% | 147.57% | 150.63% |
| SoC thermal_zone0 range | 51.254–54.332 °C | 53.036–54.818 °C |
| Sunshine RSS | 107,684–107,692 KiB | 108,912–109,552 KiB |
| Minimum MemAvailable | 410,140 KiB | 406,252 KiB |
| Minimum total CmaFree | 27,804 KiB | 28,000 KiB |

Telemetry was sampled about every five seconds. CPU/IRQ/temperature/memory summaries use steady intervals with >40 VE interrupts/s: Wi-Fi 152.91 s, USB 187.05 s. The Wi-Fi monitor began before its successful client launch, so idle samples are excluded. `summarize.py` and both summary JSON files reproduce the numbers. Total CMA includes other pools; it is not the VE pool's free space alone.

Ping loss is not RTP packet loss. The five Wi-Fi losses are post-FEC complete-frame losses; pre-FEC RTP packet loss was not measured. There were no corresponding USB frame gaps. RTT/processing figures are not display latency: this client had no display renderer.

## CPU and encoder evidence

`top -H` second samples (not its cumulative first sample):

| Link / UTC | Hot thread | Other busy threads | Sum of these samples |
|---|---|---|---:|
| Wi-Fi / 09:12:32 | TID 5633: 98.1% | 5625: 19.4%; 5630: 17.9%; 5648: 12.4%; two at 0.5% | 148.8% |
| USB / 09:16:29 | TID 6296: 99.1% | 6288: 19.9%; 6293: 18.4%; 6308: 12.0%; 6290: 0.5% | 149.9% |

Sunshine's KMS timing logs report roughly 70–72 ms capture/readback p50 and 12–13 ms conversion p50 (about 20–22 ms p95). Encode-call p50 is about 2.1 ms. These are the existing producer's probe timings, not additive end-to-end deadlines; the pipeline is parallel. Do not infer 14 fps delivery from the 70 ms timing: actual receiver counts and VE IRQs both establish about 50.5 fps.

Kernel session summaries report 9,504 encoded frames at mean **1.265758 ms VE time** for Wi-Fi and 9,716 at **1.311691 ms** for USB, at 600 MHz. Differences from received totals include startup/teardown frames. Independent moving-NV12 throughput reached **182.039 fps**. No software H.264 fallback occurred.

**Inference:** the saturated CPU thread plus capture/conversion timings, available VE throughput, and unchanged fps on loss-free USB locate the limiting path before/around CPU capture/conversion rather than the encoder or Wi-Fi bandwidth. No profiler attributed the hot TID to an exact function, so an exact CPU-symbol attribution is not claimed.

Follow-up, not implemented: investigate the **VE ISP BGRA ingest / RGB→YUV path** used by rgsp-cast, documented in worksheet section 7. It could remove software conversion. Its cost is new buffer/import and format-validation work; it does not by itself prove that KMS readback cost disappears. P frames and tighter rate control remain separate limits.

## Picture verification

A separate 30-second USB request used `H700_DUMP=/tmp/h700-sun/picture-usb.h264` with the same observer and 20000 kbps settings. It received **1,423 frames over 27.785 s**, with zero frame gaps. FFmpeg independently decoded **all 1,423 frames**, H.264 High level 4.1, **640×480**, with no decode errors.

```sh
ffmpeg -v warning -i picture-usb.h264 -f null -
ffprobe -v error -select_streams v:0 -count_frames \
  -show_entries stream=width,height,nb_read_frames,codec_name,profile,level -of json picture-usb.h264
ffmpeg -v error -i picture-usb.h264 -f framemd5 picture.framemd5
```

- `screen-reference.png`: independent compositor screenshot of the Pro's welcome screen, captured with prebuilt aarch64 grim.
- `picture-first.png`: initial dummy black picture; the first two decoded pictures were black.
- `picture-mid.png` (frame 700) and `picture-last.png` (frame 1400): correct colored Pico cartridges in different horizontal positions. They match the screen's content and colors, not garbage.
- `picture.framemd5`: 220 distinct decoded image hashes and 303 image transitions among 1,423 frames. Compression/rate-control changes can also change a hash; the visibly different cartridge positions establish motion.

This is received-network-stream verification, not a substitute hardware probe file. No display client, audio playback, or interactive Moonlight input acceptance is claimed.

## Focused checks and encoder regression

| Check | Result |
|---|---|
| Compatible module cross-build, W=1 | Pass |
| Controls probe cross-build, -Wall -Wextra -Werror | Pass |
| Actual 16/4 request → bounded 8/4; CREATE_BUFS at cap | Pass |
| STOP before STREAMON, START and OUTPUT STREAMON resume with queued work | Pass; 62/61/61 decoded frames |
| 1280×720 / 1920×1080 controls and full host decode | Pass; 62 frames each |
| 600 moving 640×480 frames, 10 Mbps | Pass; minimum Y PSNR 42.919274 dB; 9,963,371 B |
| 600 moving frames, 1 Mbps | Pass; minimum Y PSNR 31.321423 dB; 1,284,892 B |
| Sustained moving NV12 encode plus five decode rounds | 5,462 frames / 30.004561 s = 182.039 fps; 74.364% producer+FFmpeg CPU |
| Concurrent stateless decode | 5 × 120 frames, reference SHA `0c891fbb2e0740e006286f92c89aae79c19785f9e43439893ff5338c9ede6b53` |
| `nix build ...#checks.x86_64-linux.h700` | Pass; also built main-based kernel and modules |
| Verifier unit tests | 5/5 pass |
| Strict kernel checkpatch | 0 errors, 1 existing new-file/MAINTAINERS warning, 0 checks; exit 1 for that warning |
| `git diff --check` and pre-commit secret scan | Pass |

No Rust/TS code changed, so those typechecks were not applicable. No repository-wide full test suite or full verification ran. The real paths exercised were plugin update/enable/disable, authenticated pairing, two sustained Moonlight transports, received-picture decoding, and live kernel encode/decode. The focused check logs and raw regression streams are retained here.

## Final state and remaining gates

- Streams stopped; Desktop sessions ended; Moonlight client successfully unpaired.
- `korri-plugin disable @korri:sunshine` completed. New package remains selected, desired state Disabled. Native Sunshine service and certificate-control socket are inactive.
- Fixed test module unloaded; `modprobe sunxi-cedrus` restored `/run/booted-system/kernel-modules/lib/modules/7.2.0/kernel/drivers/staging/media/sunxi/cedrus/sunxi-cedrus.ko`. Both video nodes are present, as at entry. Module reference count is zero after the restore/decode test.
- Restored-module decode produced the exact 120-frame reference hash above.
- Pico kiosk PID **1385**, korrid PID **1457**, compositor PID **992**: all active, unchanged, **NRestarts=0** throughout. The normal idle policy later powered DSI-1 off again.
- Boot ID unchanged: `5b608576-8d4a-4e79-835b-712e486d0aea`.
- Extlinux SHA-256 unchanged: `c152347420130481ac4e29ba43a5f864b8a90822b713115b609497a441d8e132`.
- `/run/current-system` and `/nix/var/nix/profiles/system` still resolve to the xqxwcwj system above.
- The previously absent live `sunshine_state.json` is absent again. Test-only state was moved to a mode-0600 `/tmp/h700-sun-test-state.json` on the Pro, and temporary API credential/PIN files were removed. Existing `sunshine.conf`, apps, server certificate, and private key were not edited.

**Not accepted:** sustained 60 fps, literal warning-free execution across rejected candidates, audio, interactive client input, display latency, pre-FEC packet loss, P frames, tight CBR, or a deployed/persistent queue fix. Audio initialization logged a PulseAudio timeout and the stream had no audio. The unchanged boot-default module still lacks this new queue cap; repeating Sunshine acceptance requires proper deployment or another explicitly authorized compatible live-module test.

The parent will decide and perform deployment. No test module was left loaded to stand in for deployment.

## Landing and deployment (added 2026-10-22)

The pool-budget fix landed on `main` as `dd0f88a62` (rebased from `d879f951a`; patch 0237 unchanged). The Pro's boot default is now `/nix/store/lf7gvy9a8a2ixixfb37z1i5820babg74-nixos-system-rg35xxpro-sd-card-26.11.20260923.4975466`: its running Korri software with the landed patch 0237 and a byte-identical kernel Image. On boot `1878c621-d1ad-488e-bb13-0a312dd83a76`, Sunshine's own probe encoded on `/dev/video1`, a 600-frame motion stream verified at minimum Y PSNR 42.92 dB, a 30 s run sustained 187.96 fps, decode and idle/wake regressions passed, and the kernel log had 0 fault lines. Sunshine remains installed and Disabled; `rg35xxpro` still does not select the streaming host in `nix/product/plugin-selection.nix`.
