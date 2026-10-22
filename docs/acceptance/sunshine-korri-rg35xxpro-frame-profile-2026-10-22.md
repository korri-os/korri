# RG35XX Pro Sunshine frame profile — 2026-10-22

**Verified: KMS RAM readback is the largest cost: about 72 ms for each newly captured frame. The approximately 48–50.5 fps figure counts encoded deliveries, not newly acquired KMS frames.** Swscale is secondary. The V4L2 upload and VE wait are small. No tested change proves 60 fresh frames/s; this was measurement only.

## What was measured

All artifacts below are in `/tmp/h700-sun/` on zao. SSH used the pinned `/tmp/rg35xxpro-delivery/device.py` transport to `root@192.168.1.163:2222`. Streams used the existing USB route, zao `10.42.6.12` / `enp90s0u2u3u1` to Pro `10.42.6.1` / `usb0`. The actual executable was `/nix/store/v9v77hxz3h0syqpi2vk0pqm75x4437qs-sunshine-korri-2025.924.154138-korri/bin/sunshine`.

The accepted command shape was reused, including the already built `client-metrics.so` observer and the existing Moonlight key directory:

```sh
timeout --signal=INT --kill-after=15 93 env \
  LD_PRELOAD=/tmp/h700-sun/client-metrics.so \
  /nix/store/388lgllfhliykj646jmymximdn83a18a-moonlight-embedded-2.7.0/bin/moonlight \
  stream 10.42.6.1 -platform fake -viewonly -width 640 -height 480 \
  -fps 60 -codec h264 -bitrate 10000 -app Desktop \
  -keydir /tmp/h700-sun/client -quitappafter
```

The 93-second client timeout leaves more than 90 seconds of video after startup. Exit 124 is the scheduled clean interrupt, not a crash. The fake client counts complete received frames; it does not display them. Hardware encoder selection is independently logged as `h264_v4l2m2m` on `/dev/video1`, driver `cedrus-encoder`. No software H.264 stream was used.

| Run / status | Video / result | Evidence |
|---|---|---|
| Verified: first untraced panel-on run, 09:57:13 onward | 4,382 frames / 91.237 s; **48.018 fps**; zero missing frame numbers | `profile-on-{client,host,sunshine,kernel}.log` |
| Verified: panel off, 09:59:09 onward | 95.03 s telemetry; **no video**; 0 VE IRQ/s; capture cannot find an active monitor | `profile-off-*` |
| Verified: supplemental traced run, 10:01:18 onward | 4,131 frames / 91.410 s; 45.181 fps overall; ptrace lowers throughput | `profile-trace-*`, `profile-{all,capture,encode}.strace` |
| Verified: 31.569 s control without server sampler or ptrace | 1,523 frames; **48.212 fps**; zero gaps | `profile-clean-client.log` |

The old 50.410 fps USB result was not exactly reproduced. The present untraced result is about 48 fps. The short unsampled control rules out the telemetry sampler as the main cause of that difference. Why this boot/run is about 2 fps lower remains **unknown**. No setting was changed to force the old number.

**Verified motion:** the first run also saved `profile-on.h264`. Independent FFmpeg decode read all **4,382** 640×480 H.264 frames without decode errors. I inspected `profile-motion-01.png` (frame 500) and `profile-motion-02.png` (frame 1500): the Pico cartridge tiles occupy different horizontal positions, and the prompt changes visibility. See `profile-motion-verify.log`.

## Ranked frame budget

**Do not add the capture-thread budget to the encoder-thread budget. They run in parallel. Also do not charge conversion to every encoded repeat.** Sunshine's existing compiled-in timing probes measure 300 successful captures/conversions per report. Their exact scope is established by the installed derivation's patch 0033, not inferred from the log label alone.

### Per newly acquired KMS frame (first untraced 90-second stream)

| Rank | Stage | Measured wall time | Confidence / evidence |
|---:|---|---|---|
| 1 | EGL/GL readback into Sunshine's heap image | p50 **71.45–73.25 ms**, p95 **81.98–88.81 ms** across four 300-frame batches | **Verified** producer probe around `GetTextureSubImage`; rotation is false, so no rotation draw |
| 2 | RGB/BGR0 → NV12 swscale conversion | p50 **14.53–14.99 ms**, p95 **23.52–26.48 ms** | **Verified** producer probe around `session->convert`; unchanged 640×480 means no aspect-ratio padding copy |
| 3 | Other capture work | about **0.5–0.6 ms** difference between capture and readback p50 | **Inferred estimate**, not a paired per-frame subtraction; total capture p50 **72.07–73.84 ms** is verified |

**Verified:** 900 newly captured frames between consecutive probe-report boundaries took 67.344 s: **13.364 captures/s**. Conversion report boundaries follow the same 300-frame batches. This explains why a 72 ms capture call can coexist with 48 fps network delivery.

**Verified source behavior:** `video.cpp` waits at most `1000/60 = 16.667 ms` for a new image. On timeout it re-encodes the previous AVFrame without another conversion. **Inferred:** roughly **72%** of encoded deliveries during the stable window reuse the previous captured image. This is a producer-rate estimate, not a claim that 72% of compressed frames have identical bytes or hashes.

### Encoder loop, per encoded delivery

The first run's period is **20.8255 ms**. Existing fresh-frame encode-call p50 is **2.156–2.201 ms**, p95 **3.604–4.595 ms**, including buffer upload, encoder submission, wait/receive, and packet handling. These aggregate times must not be added again to the sub-stages below.

| Rank | Stage | ms per encoded delivery | Confidence / scope |
|---:|---|---:|---|
| 1 | Waiting for a fresh capture / repeat timeout | **13.52 average** in the scoped trace; timeout p50 **16.736** | **Verified traced wait duration**, not an untraced baseline mean; source establishes this is the image queue, not VE wait |
| 2 | Conversion, amortized over deliveries | about **4.12** | **Inferred**: 14.8 ms × 13.364 captures/s ÷ 48.018 deliveries/s; actual per-fresh-frame timings above |
| 3 | VE work | **1.267 mean** | **Verified** kernel's complete first-session report: 4,384 encoded frames, 1,266,974 ns VE mean at 600 MHz; includes startup/teardown frames |
| 4 | NV12 upload plus immediate user-space bookkeeping | p50 **0.646**, p95 **0.950**; mean **0.700** | **Inferred upper bound on pure copy**, from last zero-time OUTPUT poll return to OUTPUT QBUF entry; includes ptrace/scheduling overhead |
| 5 | OUTPUT submit ioctl | p50 **0.173**, p95 **0.791** | **Verified scoped strace**; instrumentation raises this cost |
| — | Receive wait corroboration | poll p50 **1.149**, p95 **1.660** | **Verified scoped trace**; overlaps VE work above, do not add it again |
| — | Parallel capture cost, amortized over deliveries | about **20.07** | **Inferred** 72.1 × 13.364 / 48.018; occupies a different thread and is not part of the encoder-loop sum |

**Unknown:** the exact pure `memcpy` duration isolated from all surrounding instructions. The trace does not intercept function entry/return. It does bound the upload well below capture/conversion. Claiming a fully exact, additive five-stage budget would exceed the evidence.

## Capture method, framebuffer and memory

| Claim | Status / direct evidence |
|---|---|
| Active scanout is **640×480 XR24/XRGB8888, linear modifier 0, pitch 2560**, plane 41 / CRTC 49 | **Verified** `profile-drm-info.json`, live atomic state in `profile-config-source.log`, and live GETFB2 from `profile-readbench.log`. FB IDs rotate among Sway buffers. |
| Actual path is **KMS GETFB2 → PRIME export → EGL/Panfrost import → GL GetTextureSubImage CPU readback into heap → swscale NV12 → memcpy into V4L2 MMAP OUTPUT** | **Verified** source plus scoped trace. This is neither direct scanout-to-VE zero-copy nor just a Sunshine `mmap` handed to the encoder. |
| Mesa maps the imported 1,245,184-byte allocation through `/dev/dri/renderD128` each capture, then unmaps it | **Verified** capture trace; 1,228,800 bytes are visible pixels. No encoder DMABUF import occurs; V4L2 buffers report `V4L2_MEMORY_MMAP`, OUTPUT length 460800. |
| The big delay is user-space readback, not waiting in a DRM ioctl | **Verified**: scoped trace has p50 **73.322 ms** between PANFROST_WAIT_BO return and subsequent teardown; WAIT_BO itself p50 **0.037 ms**. Capture ioctl median **0.035 ms**. |
| Scanout CPU mapping uses write-combined, not ordinary cached, memory | **Inferred, strongly grounded in the exact running kernel source**: sun4i uses the DMA GEM helpers with default `map_noncoherent=false`; allocation/mmap use `dma_alloc_wc`/`dma_mmap_wc`; Panfrost's imported shmem mmap delegates back to the exporting dma-buf. Actual PTE memory attributes were not directly read. |

`profile-cache-source-identity.log` compares the relevant helper C files byte-for-byte with the **actual running kernel derivation's Linux 7.2 tarball**. Its only patch touching `sun4i_drv.c` adds H616 compatibility; it does not replace GEM allocation/mapping. The exact derivation is preserved in `profile-kernel-derivation.json`. This is not an assumption based on a different kernel's headers.

### Read-only bulk-read control

To distinguish raw memory bandwidth from GL's access path, a small read-only GETFB2/PRIME/mmap profiler was cross-built on **zao**, with `-O2 -Wall -Wextra -Werror`, signed with the existing cache key and nix-copied. It never writes scanout or changes modes. Source, Nix expression and build log are `profile-readbench.{c,nix}`, `profile-readbench-build.log`. Binary: `/nix/store/p1lk17liaj3z0hiai1ym9dmlsckk59fl-h700-sunshine-readonly-scanout-profiler-aarch64-unknown-linux-gnu/bin/readbench`.

| 30 × 1,228,800-byte memcpy, two runs | Scanout dma-buf → heap | Cached heap → heap |
|---|---:|---:|
| Verified mean | **13.001 / 13.249 ms** | **1.087 / 0.935 ms** |
| Verified p50 | **12.747 / 13.129 ms** | **1.094 / 0.927 ms** |
| Verified throughput | **94.52 / 92.75 MB/s** | **1130.69 / 1314.30 MB/s** |

These were separate post-stream controls; governor stayed `schedutil`, observed frequency rose from 1.104 to 1.320 GHz. They are not concurrent-stream timings or a coherent-frame capture correctness test. **Inferred:** slow CPU access to scanout matters, but raw memcpy can read it about **5.5× faster** than the existing GL readback. Therefore WC memory alone does not account for every one of the 72 ms; GL's format/access path and its CPU read pattern are important. A cached/engine path is promising, but this control does not prove its sustained 60 fps performance.

## CPU, IRQ, frequency and thermal evidence

The sampler reads every Sunshine TID's `/proc/<pid>/task/<tid>/stat`, `schedstat` and `wchan`, and records its kernel thread name. One core is 100%. CPU is the difference in user+system ticks divided by actual elapsed uptime; `CLK_TCK=100`. CSVs contain every per-thread sample, including zero-CPU threads. Nothing relies on the misleading cumulative first `top` sample.

The initial panel-on run sampled every **1.079 s** (range 1.05–1.13 s). The sampler was then corrected to an absolute one-second schedule; the panel-off run, supplemental trace run, and final untraced 90-second run use that schedule. See the final run section below.

| First untraced run, steady intervals | Verified result |
|---|---:|
| CPU total | **165.82%** |
| Hottest TID 3478, name `sunshine` | **96.15%** CPU, 95.06% user |
| TIDs 3470 / 3475 / 3484 / 3508 | **18.40 / 17.89 / 11.85 / 11.67%** |
| VE IRQ rate, 90.82 s steady intervals | **47.941/s** |
| Governor / actual and requested CPU frequency | **schedutil / 1,416,000 kHz throughout steady samples** |
| thermal_zone0 | **49.958–54.008 °C** |
| All four zones combined | **49.553–57.005 °C** |
| CPU and GPU thermal cooling state | **0 throughout** |

**Verified role attribution in the supplemental stream:** TID 4256 performs DRM capture and is the hot thread. TID 4255 runs V4L2 OUTPUT and CAPTURE receive. TID 4250 requeues CAPTURE buffers after packet consumption. TID 4259 is the conversion worker (**inferred worker role**, based on futex pairing and source). The corresponding role labels for the first run's differently numbered TIDs are **inferred**, not function-profiled identities.

**Verified:** swscale is **already configured with two threads**. The actual build's `video.cpp` sets its `threads` option from `config::video.min_threads`; the unchanged empty configuration uses `config.cpp` default **2**. A second worker is observed. This is not a single-threaded converter awaiting the obvious multithreading switch.

**Verified ioctl rate during the scoped trace:** 340 OUTPUT QBUFs, 340 OUTPUT DQBUFs and 340 CAPTURE DQBUFs; **43.051 encoded submissions/s**, **129.30 encoder-thread ioctls/s**. CAPTURE QBUF occurs on the packet-consumer thread, once per frame; four V4L2 queue/dequeue ioctls per encoded frame implies about **172/s** in that traced window and **192/s at the untraced 48 fps rate**. The latter rate is **inferred**, not an uninstrumented ioctl count. Capture does about **27 DRM ioctls per acquired frame**; scoped capture GETFB2 rate **12.177/s** and total ioctl rate **331.97/s**. Ptrace lowers throughput; use the receiver and IRQ counts for baseline throughput, not these tracing-window counts.

**Verified:** no sampled CPU-frequency cap reduction or thermal cooling intervention occurred. The kernel journal contains no WARN/Oops/BUG/encoder timeout/IOMMU fault in this measurement window. **Unknown:** sub-second clock or thermal events between samples; this is not an exhaustive hardware throttle-counter proof.

## Panel off is not another functioning capture mode

**Verified:** `swaymsg ... output DSI-1 power off` removes the active KMS CRTC/FB. The same 640×480/60/10000 request then cannot start video. Sunshine logs `Couldn't find monitor [0]`, followed by `Unable to find display or encoder during startup` and `Please check that a display is connected and powered on`. No encoder actually ran. This is not 60 fps black-frame streaming or a successful low-cost capture optimization.

Across **95.03 s**, every sampled DPMS value was Off, VE interrupts stayed unchanged, Sunshine CPU averaged **0.410%**, CPU frequency ranged **480–1416 MHz**, all-zone temperature **45.989–50.606 °C**, and both cooling states stayed 0. All per-thread samples remain in `profile-off-threads.csv`. Capture/conversion/upload/encode times are **not applicable**, not zero-cost successful stages.

## Which follow-up could reach 60?

| Option | Assessment | Cost / what it does not solve |
|---|---|---|
| (a) VE ISP RGB ingest, removing conversion **only** | **Verified insufficient alone:** it leaves the approximately 72 ms capture/readback and roughly 13 fresh captures/s. | New format/driver work; 48–53 repeated deliveries/s is not 60 fresh frames/s. |
| (b) Faster readback, caching/engine import, true zero-copy | **Inferred necessary first target.** The 13 ms direct bulk-read control shows substantial avoidable GL overhead; avoiding CPU readback altogether is stronger. | Correct buffer lifetime, cache synchronization, format and modifier handling are required. Existing encoder accepts NV12, not a magically interchangeable XR24 dma-buf. Faster readback alone still leaves approximately 15 ms conversion plus 2.16 ms encode, already near/over the 16.667 ms serial encoder deadline. No 60 fps guarantee. |
| (c) Multithreaded conversion | **Verified already enabled at two threads; insufficient alone.** | More threads may consume CPU for limited gain and do not remove the capture bottleneck. |
| (d) Pacing / queue change | **Inferred can lift the encoded-repeat rate**, because every empty-queue loop waits another 16.667 ms before upload/encode. | It can make 60 repeated packets/s without delivering any more real motion. It cannot fix the 72 ms capture path. |
| Combined (b) + RGB conversion in the engine / (a) fed from scanout, not after RAM readback | **Inferred strongest route to 60 fresh frames/s.** It removes both the proven dominant readback and the secondary conversion from the CPU critical path. | Requires implementation and a new 90-second capture-rate/receiver-rate test. This measurement does **not** verify that path or promise 60 fps before it exists. |

## Safety and reproducibility

- **Verified:** no repository edits, commits, pushes, lands, reboot, boot-default change, module swaps, target compilation, permission widening, or Pico/korrid/compositor restarts. No full project test suite, linter or typecheck was applicable to repository code because none changed.
- The live Sunshine state file was absent at entry despite the client retaining pairing keys. The saved `/tmp/h700-sun-test-state.json` was temporarily copied back to reuse the existing authenticated pair. Sunshine alone was stopped/started to load it. The temporary live file was removed after disabling the plugin; the saved state and zao client keys were kept. No new pair, administrator or permissions were created.
- **Verified:** `sunshine.conf` stayed the original **zero-byte file** throughout. No logging configuration change was needed: this binary already has `SUNSHINE_CAPTURE_ROTATION_PROBE` enabled, and info-level probes are sufficient. `cmp` against the entry backup passes; SHA-256 remains `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
- `drm_info` was a prebuilt signed/copied Nix artifact. The read-only scanout profiler was compiled only on zao. Added tool store closures and profiling files remain as measurement artifacts; no executable or service was replaced.
- `profile-run.py`, `profile-monitor.sh`, `profile-analyze.py` and `profile-trace-analyze.py` reproduce the collection/summaries. Python byte-compilation and `bash -n` pass. The C profiler's warning-as-error cross-build and its two actual device runs pass. `git status --short` was empty.
- An initial client launch incorrectly put the observer in `timeout`'s environment and failed symbol lookup before streaming. It is preserved as `profile-on-client-preload-failure.log`; it is excluded from results. Successful commands apply LD_PRELOAD only to Moonlight.

Focused artifact assertions pass: each run has 96 samples; CSVs contain 2,440 / 1,520 / 2,518 / 2,517 thread-interval records for on / off / trace / main, with all 27 / 16 / 27 / 27 observed TIDs represented. All intervals are positive; final on/off schedules stay below 1.1 s. Final service PIDs, Disabled state, boot ID and boot-file hash assertions pass. Final protected-service state and boot/config identity are recorded in `profile-final.log`; see the final-run appendix below.

## Final untraced run: one-second sampling and final state

**Verified:** a second untraced run, `profile-main-*`, started at **10:12:33 UTC** and delivered **4,401 frames over 91.312 s = 48.186 fps**, with zero missing frame numbers. It used the identical USB/640×480/60/10000 command, without the H.264 dump or ptrace. The 96 telemetry samples have **1.00021 s mean spacing**, range **0.98–1.02 s**. The complete per-second thread/name/CPU data is `profile-main-threads.csv`; source records are `profile-main-host.log`.

| Final-run measurement | Verified result |
|---|---:|
| VE IRQ rate, 91 s steady intervals | **48.110/s** |
| Sunshine CPU total | **166.03%** |
| CPU governor / frequency | **schedutil / 1416 MHz** throughout steady samples |
| SoC thermal_zone0 | **51.902–55.061 °C** |
| All-zone temperature range | **51.902–57.896 °C** |
| CPU / GPU cooling state | **0 / 0** throughout |
| Capture p50 across four batches | **72.142–73.779 ms** |
| GL readback p50 | **71.566–73.065 ms** |
| Conversion p50 | **14.479–14.982 ms** |
| Encode-call p50 | **2.143–2.166 ms** |
| Kernel VE mean, 4,406 encoded frames including startup/teardown | **1.267056 ms at 600 MHz** |

**Verified:** final-run probe boundaries give **13.504 fresh captures/s**. The conclusion and ranking above are reproduced, not dependent on the first sampler’s slightly longer intervals.

Every observed TID is listed below. CPU values are steady-window means; names are the kernel’s actual thread names, not invented function names.

| TID | Thread name | CPU % |
|---:|---|---:|
| 7485 | `sunshine` | 96.571 |
| 7479 | `sunshine` | 18.473 |
| 7484 | `sunshine` | 17.703 |
| 7488 | `sunshine` | 11.835 |
| 7505 | `sunshine` | 11.679 |
| 7497 | `threaded-ml` | 3.845 |
| 7480 | `sunshine` | 3.670 |
| 7483 | `sunshine` | 1.604 |
| 7481 | `sunshine` | 0.407 |
| 6960 | `sunshine` | 0.154 |
| 7080 | `sunshine` | 0.044 |
| 7138 | `sunshine` | 0.022 |
| 7482 | `sunshine` | 0.022 |
| 7041 | `sunshine` | 0.000 |
| 7042 | `sunshine` | 0.000 |
| 7133 | `sunshine` | 0.000 |
| 7134 | `sunshine` | 0.000 |
| 7135 | `sunshine` | 0.000 |
| 7136 | `sunshine` | 0.000 |
| 7139 | `sunshine` | 0.000 |
| 7140 | `sunshine` | 0.000 |
| 7141 | `sunshine` | 0.000 |
| 7143 | `[pango] fontcon` | 0.000 |
| 7144 | `pool-spawner` | 0.000 |
| 7145 | `gmain` | 0.000 |
| 7147 | `gdbus` | 0.000 |
| 7495 | `sunshine` | 0.000 |

### Final cleanup — verified on the Pro

- `korri-plugin disable @korri:sunshine` completed. Desired state is **Disabled**. Sunshine and its certificate-control socket are inactive. The temporary live pairing state is absent again. The configuration compares byte-for-byte with the entry backup.
- **Pico/Chromium kiosk:** `korri-chromium-kiosk`, PID **1302**, active, **NRestarts=0**, activation monotonic **48709810**, unchanged from entry.
- **korrid:** PID **1447**, active, **NRestarts=0**, activation monotonic **34129917**, unchanged.
- **Compositor:** PID **1005**, active, **NRestarts=0**, activation monotonic **28488607**, unchanged. The idle service is active, PID **1293**, **NRestarts=0**.
- DPMS is **On**, as at entry; the normal idle policy remains running.
- Boot ID remains **1878c621-d1ad-488e-bb13-0a312dd83a76**. Booted/current/system profile paths all remain `/nix/store/lf7gvy9a8a2ixixfb37z1i5820babg74-nixos-system-rg35xxpro-sd-card-26.11.20260923.4975466`.
- Extlinux SHA-256 remains **d7ad843d83a272249bf5c0a078c137cf925a7eed232e8da923d34f2ddb1a61d0**. No reboot or module load/unload command was run. Cedrus reference count is 0 after cleanup; kernel taint is 1024 (staging driver), with no new WARN taint.
- Full final checks and kernel-window messages are in `profile-final.log`. The measurement files and additional signed profiling-tool closures remain; no shared-store garbage collection was attempted.
