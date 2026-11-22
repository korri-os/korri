# Tab S6 game-start audio stall: read-only investigation

## Result

**Verified:** the freeze propagates from the ALSA/q6asm playback path through
PipeWire and PulseAudio to RetroArch's emulation thread. **Not verified:** the
physical TDM DMA engine stopped. q6asm reports completed buffer indices, not a
hardware DMA position.

The leading fix candidate is upstream Linux
[`54e89fab723d5a374972d3f5c3e0b89ae055bd18`](https://github.com/torvalds/linux/commit/54e89fab723d5a374972d3f5c3e0b89ae055bd18),
“ASoC: qcom: q6asm: Reset PCM positions when preparing a stream”. It is absent
from the tablet's 7.3.4 source. The five-line fix is backported unchanged for
the Korri product kernel only. It fixes a reproduced driver accounting defect;
its connection to this particular game launch remains a hypothesis. **Do not
claim that the device stall is fixed until the experiment below passes.**

No playback, ALSA control writes, debugger attachment, process termination,
service restart, reboot, or deployment was performed on the tablet. The GPU
agent retained runtime control. Live capture ended at about 10:18:45 MST.

## Evidence

Raw captures are on zao in `/tmp/tabs6-audio-evidence/`. The frozen boot was
`ef171344-8dce-415b-ac6d-54f8f54c4456`, Linux 7.3.4, on 2026-11-22.

| Observation | Evidence file |
| --- | --- |
| All four PCM substreams, hardware/software parameters, two interrupt samples, process list and audio/RetroArch kernel stacks | `final-proc-stacks.txt` |
| Full boot journal, including audio initialization and launch | `final-journal.txt` |
| Kernel ring at capture end; early boot messages had already rolled out | `final-dmesg.txt` |
| PipeWire graph and client properties; status and three pw-top samples | `final-pw-dump.json`, `final-pw-status.txt` |
| Earlier complete kernel journal and audio service journal | `current-journal.txt` |
| ALSA fd owner, timerfds, audio thread stack, clock summary | `fd-clock.txt` |
| DPCM state, GPIO mux, amp DAPM paths, trace support | `asoc-config.txt`, `pins-asoc.txt`, `amps-history.txt`, `trace-support.txt` |
| The GPU agent's original backtrace, stalled PCM, launch and working-boot PCM samples, copied without alteration | `sysmem2-stuck.txt`, `sysmem2-audio.txt`, `sysmem2-launch.txt`, `audio-drain.txt` |
| Kernel messages from the working and two subsequent boots | `compare-boots.txt` |
| Upstream patch and commit metadata | `q6asm-reset-upstream.patch`, `q6asm-reset-commit.json` |

### Stream and owner

| Layer | Verified identity/state |
| --- | --- |
| Game | Super Mario World (USA), Snes9x 1.63 through RetroArch 1.22.2, launched by korrid |
| Emulation process | Host PID 2036; main thread waits in `pa_threaded_mainloop_wait` → `pulse_write` → `audio_driver_sample_batch` → Snes9x `retro_run` in the GPU agent's saved backtrace |
| Pulse client | PipeWire client 52, `application.name=RetroArch`, `client.api=pipewire-pulse`; node 53, F32LE stereo 48 kHz; both channel links active to node 50 |
| ALSA owner | `korri`'s PipeWire PID 1191, fd 51 → `/dev/snd/pcmC0D0p` |
| ALSA playback | Card 0 `SMT860`, `MultiMedia1`, `hw:SMT860,0`, `pcm0p/sub0`; all three other PCM substreams closed |
| Backend | DPCM frontend and `Secondary TDM Playback` both report `State: start`; UCM routes `SEC_TDM_RX_0 Audio Mixer MultiMedia1` |
| Format/geometry | MMAP_INTERLEAVED, S16_LE, 2 channels, 48000 Hz, period 480 frames (10 ms, 1920 bytes), buffer 3840 frames (8 periods, 80 ms) |
| PipeWire sink | Node 50 `alsa_output.platform-sound.HiFi__Speaker__sink`; period 480, period count 8, headroom 480, suspend timeout 0 |

The PCM remains `RUNNING` with `hw_ptr=480`, `appl_ptr=2016`, `delay=1536`,
`trigger_time=71.554539088`, and `tstamp=71.554592525`. These values match the
GPU agent's original capture, the 10:05 capture, and the 10:18 capture. The
final pair is two seconds apart. `pw-top` shows running nodes but no graph
cycles (quantum/rate 0). PipeWire's data thread waits in epoll; RetroArch's main
thread waits on a futex. There is no blocked kernel ioctl in those samples.

On the earlier working boot (`699953d8514b44f69f05a2f7e0e1facf`), the same
480-frame/3840-frame geometry advances `hw_ptr` from 1061280 to 1158720 in
about 2.030 seconds: 97440 frames, consistent with 48 kHz.

### Interrupts, DSP and speakers

* ADSP remoteproc reports `running`. Its watchdog and fatal counters are zero.
* ADSP's GLINK interrupt is IRQ 183 / GIC SPI 188. It remains at **402** in
  both final interrupt samples, as it did in the earlier samples. This is a
  shared message interrupt, not a dedicated PCM period interrupt.
* The four amps share IRQ 188 / GPIO 133; its count is **0**. This is an amp
  fault/event interrupt, not the playback period clock. Zero does not prove a
  broken amp or absent TDM clock.
* GPIO 126–129 belong to `sound`, function `sec_mi2s`, not the inherited
  reserved-GPIO configuration described in the reference port's early failures.
* All four amps' DAPM `AMP Playback`, `ASPRX1`, `PCM Source`, `Main AMP`, and
  `SPK` paths are on. No amp `Enable(1) failed: -110` appears in this boot.
* The generic q6afe clock-provider summary lists unused 19.2 MHz placeholder
  clocks, including SEC_TDM_IBIT. The machine driver requests the TDM clock
  through the DAI/APR path, not those clock-provider consumers. That summary
  does **not** measure the physical BCLK. No physical clock measurement exists.

### What the kernel logged

The full frozen-boot kernel journal has normal ADSP startup at 8.420 s, APR
service registration at 9.926–9.928 s, and four CS35L41 revision-A0 probes.
Each amp prints `Subsystem ID not found` at probe. There are **no** q6asm,
q6afe, q6adm or q6routing errors, `Memory_map_regions failed`, audio start
allocation failures, amp enable timeouts, or reported audio xruns around the
71.554 s trigger or later capture. The successful boot has the same amp probe
messages. Other logs include UFS query failures and network timeouts; these
are not audio timeout evidence.

PipeWire logs `hw:0,1: capture open failed: Invalid argument` at 16.148 s.
That is the separately configured microphone node, marked `nofail`; capture
is closed while the playback sink and RetroArch links remain present.
WirePlumber also logs RTKit D-Bus timeouts at 18.352 and 20.355 s. They do not
by themselves explain a non-advancing q6asm completion pointer.

PipeWire 1.6.8's `spa/plugins/alsa/alsa-pcm.c`, saved as `alsa-pcm.c`, also
matters: `alsa_recover()` calls `do_drop()`, `do_prepare()`, and `do_start()`
on the same `state->hndl`. Its xrun message uses trace logging, below the
captured default log level. Thus a startup xrun can exercise the broken kernel
reprepare path without an xrun line in this journal. That sequence is possible,
not observed in this boot.

The absence of an audio error is not proof that DSP RUN or WRITE succeeded:
`q6asm_run_nowait()` is asynchronous, and the current callback does not report
RUN status as an error to PCM userspace. It also discards basic responses for
`ASM_DATA_CMD_WRITE_V2`. Those are diagnostic gaps in the existing driver.

## Why this is not established as the reference port's first-open failure

The reference `gts6l-mainline-linux/docs/AUDIO.md` section 5b describes a
**prepare failure**: DSP error[1], memory-map failure, `rc=-22`, then ASoC
prepare `-12`. Its section 6b also describes first-open ADM errors. Neither
signature appears in the captured Korri boot. This PCM reached RUNNING.

Both problems involve q6 audio initialization, but the saved evidence does
not establish that they are the same defect. A warm-up that just writes to
PipeWire would also leave its permanently awake, stuck sink open. It is not
a demonstrated recovery procedure here.

## Ranked hypotheses and proposed fix

| Rank | Hypothesis | Evidence and limit |
| --- | --- | --- |
| 1 | Stale q6asm queue/submission/completion positions after a drop/prepare or xrun recovery | The shipped code never resets `queue_ptr` in prepare, nor `dsp_buf`/`hw_ptr` when allocating a new ring. Upstream independently fixed this exact defect. The extracted real prepare/map/ack functions reproduce accepting a second commit without submitting the new periods. A stale completion index also explains a nonzero pointer immediately after a fresh trigger. **The captured launch's prepare history is unavailable, so attribution remains inferred.** |
| 2 | DSP stream/AFE start failure or ordering problem after initial setup | No further ADSP messages arrive, and asynchronous RUN/WRITE failures are poorly exposed. RUNNING and DPCM `start` are software states, not DSP proof. Could explain a true first-prepare failure even after the proposed reset fix. |
| 3 | TDM BCLK or backend clock/power sequencing failure | Possible without measuring BCLK, but the correct pinmux, powered amp paths, absent amp timeout, and working boot with identical geometry weaken it. The slot-width inconsistency in the reference prose is not evidence of this intermittent stall. |
| 4 | PipeWire period/headroom incompatibility | The working boot uses the same period and buffer sizes. They satisfy the driver's 480-frame step. Increasing latency or forcing IRQ scheduling is not supported by the present evidence. |

The candidate backport makes only these functional changes:

```c
/* q6asm_dai_prepare(), before mapping the new ring */
prtd->queue_ptr = 0;

/* q6asm_map_memory_regions(), when allocating the new ring */
ac->port[dir].dsp_buf = 0;
atomic_set(&ac->port[dir].hw_ptr, 0);
```

It changes neither UCM, amp gain, BCLK, slot maps, nor WirePlumber policy.
The console kernel remains unchanged. Cost: a new product kernel build and
a later controlled device test. It does not solve a DSP readiness or physical
clock problem. Drop the backport once the pinned kernel includes it.

## One proposed runtime experiment — not executed

Have the GPU agent run **one controlled, same-handle PCM reuse A/B test** on
the baseline kernel and this kernel, with no other image/configuration change.
It needs an exclusive audio window and a kernel switch; this investigation
must not perform either.

1. The GPU agent releases the playback device from the normal audio owner.
   Use one native ALSA probe, built on zao, that opens `hw:SMT860,0` **once**.
   Configure MMAP_INTERLEAVED, S16_LE, stereo, 48000 Hz, period 480, buffer
   3840, `avail_min=480`, and explicit start (`start_threshold=boundary`).
2. Prepare, mmap-commit 2016 frames of **silence**, explicitly start, then
   replenish 480 frames at a time for one second. Bound each wait to 500 ms
   and each phase to three seconds. Record PCM state, returned errors, and
   `/proc/asound/card0/pcm0p/sub0/status` throughout.
3. Call `snd_pcm_drop()` followed by `snd_pcm_prepare()` on that **same open
   handle**, with no hw-params renegotiation and no close/reopen. Repeat the
   identical playback phase ten times. Retain the kernel journal. A pair of
   `aplay` commands is not equivalent: each allocates a fresh audio client.
4. Repeat the identical probe with only this five-line backport applied.
   Baseline fresh playback must advance; a baseline reused phase must stall,
   miss submissions, or return the upstream `Error queuing playback buffer
   -16` signature. The patched fresh and reused phases must all advance.

That A/B result would confirm the ring-reuse defect on this board and the
backport's remedy. If baseline fresh playback already stalls, or both kernels
stall identically, the hypothesis fails to explain that stall: investigate
RUN/WRITE responses and AFE/BCLK next, rather than adding a boot warm-up.
Even a passing probe does not prove the game made a second prepare; verify the
original SMW launch afterward before describing the game freeze as fixed.
The current kernel has neither FTRACE nor KPROBES, so no existing trace can
recover that missing history from this boot.

## Offline validation

The new `tabs6-q6asm` check compiles the actual patched driver's
`q6asm_dai_prepare`, `q6asm_map_memory_regions`, and `q6asm_dai_ack` with an
in-memory transport. It tests fresh playback, same-handle prepare with a
partial period, subsequent partial/full-period commits, and direction-local
ring reset. It does not emulate DSP timing, DMA, speaker clocks, or PipeWire.

* The 7.3.4 source without the backport fails the second-commit submission
  assertion. The same check passes against the source produced by Nix with
  this backport.
* `nix build .#checks.x86_64-linux.tabs6-q6asm --no-link` passes, including
  application of the upstream patch to the actual product source.
* Ruff lint/format and BasedPyright pass for the regression check. Nix
  formatting passes for the new check and device composition. The existing
  `kernel/default.nix` fails nixfmt on both the baseline and this change; its
  unrelated formatting is left intact. Diff whitespace checks exclude the
  unchanged upstream patch's mandatory context-line whitespace.
* Full product-kernel build status is in the investigation report.
* No hardware acceptance test or deployment was performed.
