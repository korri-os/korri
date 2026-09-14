# R36T Max autonomous hardware survey — 2026-09-14

## Stage 1: read-only scope and baseline

This first stage preceded the separately authorized follow-up below.

Read-only inspection of the running handheld, passive sampling and low-rate
ICMP probes. No reboot, suspend, browser/service restart, governor change,
voltage change, regulator toggle, input injection, audio playback, raw register
access, storage write test or target-side build was performed.

- Kernel: Linux 6.12.63; boot ID `d34efbf7-915d-4b39-9795-1bb2083b13d5`.
- Runtime system: `86nzj98zyqx0w2cz6ikw4xzgb5gwjnkn` diagnostic closure.
- This is the temporary portal activation. It is not the SD boot default.
- Main samples cover approximately 5,194–5,532 seconds of this boot, followed by
  a final read-only check at 6,075 seconds. Device wall-clock history is
  unreliable; log filenames below use the collecting host's clock.

## Thermal limit reached before stress testing

The initial survey reported SoC temperatures of 84.6–86.5°C and GPU temperatures
of 83.1–84.2°C. Thirteen passive samples over approximately 67 seconds reported
SoC 82.7–85.8°C and GPU 83.1–85.4°C. These are driver-reported die temperatures,
not measurements of the battery or enclosure.

The kernel exposes passive trips at 70°C and 85°C and a critical trip at 115°C.
CPU and GPU cooling devices changed state during observation. This demonstrates
that software thermal management is active, not that physical temperature
calibration or emergency shutdown is correct. Sequential frequency/limit reads
are not atomic; apparent momentary disagreements do not establish a broken cap.

`cpufreq-dt` is running the performance governor, with 600, 816, 1008, 1200 and
1296 MHz OPPs available. GPU devfreq uses simple_ondemand, with 200, 300, 400 and
480 MHz available. No policy was changed.

In a separate 10.21-second cgroup sample:

| Service | CPU time consumed | Average CPU cores used |
|---|---:|---:|
| Browser | 17.252280 s | 1.69 |
| Compositor | 1.049687 s | 0.10 |
| korrid | 0 s | 0 |
| Sunshine | 0.040412 s | 0.004 |

The browser is the dominant measured userspace load. This is not an isolated
hardware-idle baseline and does not prove the temperature's cause. Synthetic
CPU/GPU load, bulk network traffic and storage stress were withheld. A temporary
browser pause and a physical check of placement/charging are the next useful
steps before increasing load.

## Memory and graphics

- MemTotal: 1,005,444 kB; MemAvailable approximately 451–465 MiB across the
  survey and passive samples. No swap; kernel `oom_kill` remains zero.
- Survey cgroup memory: browser about 259 MiB, compositor 100 MiB, korrid 2.9 MiB.
  Do not add individual RSS values to estimate total usage: mappings are shared.
- Browser, compositor and korrid had zero restarts. Browser invocation remained
  `f99c220dff994b36aa2979f53137aa8c`.
- DSI-1 is connected/enabled; backlight reports 833/1666 with power on. This does
  not test brightness range or blank/wake behavior.
- Panfrost identifies Mali-G31. Chromium and Sway have Panfrost DRM clients;
  a Chromium client reports resident GPU memory. This is evidence beyond a
  device node existing, but not a graphics benchmark or video-decode test.
- Hantro registers PX30 encoder `/dev/video0` and decoder `/dev/video1`.
  Registration alone does not prove a working encode/decode path.

## Wi-Fi and storage

RK915 is bound to both SDIO functions. The radio runs on the physical SDIO
controller at 25 MHz, four-bit, 1.8 V signaling. `iw` reports power saving on;
no power-saving setting was changed. One link snapshot reported 2462 MHz,
−34 dBm, RX 58.5 Mbit/s and TX 65 Mbit/s. These are reported link rates, not
measured application throughput.

A host-to-handheld check sent 600 ICMP probes at two per second over five
minutes: 562 replies, **6.33% loss**, RTT min/average/max 3.098/63.672/5518.662 ms.
SSH remained reachable. Reported interface RX/TX error counters remained zero,
RX drops stayed at nine and TX drops at zero across the survey/follow-up.
Those counters do not cancel the observed end-to-end packet loss. This test
fails a clean low-rate connectivity expectation; it does not isolate radio,
firmware, AP, power-saving, thermal/load or host-side causes.

A subsequent concurrent one-minute control sent 120 probes each to the handheld
and LAN router from the same host. Both returned all 120. Handheld RTT averaged
5.145 ms (maximum 19.462 ms); router RTT averaged 0.482 ms (maximum 1.842 ms).
The original stalls were not reproduced in this shorter control. They remain
unexplained, not fixed. The live firewall accepts ICMP echo requests without
an explicit per-rule limit. Neither control nor rule inspection isolates the
cause of intermittent loss.

The root filesystem is ext4 on `/dev/mmcblk0p2`, on physical SD controller
`ff370000`. The card reports 53,739,520,000 bytes. The root partition is about
49.9 GiB and the filesystem reports 52,650,274,816 bytes, with about 46.6 GB
available. Filesystem expansion to the reported card size is confirmed;
physical integrity of every advertised block is not.

SD runs at a reported 150 MHz, four-bit SDR104, 1.8 V signaling. The bounded
kernel-log review found no MMC timeout/I/O, ext4 error, GPU fault/timeout or OOM
messages matching the query below. No new read/write benchmark or
power-loss test was run. Internal eMMC remains disabled and untouched.

The executed error-only query was:

```sh
journalctl -k -b --no-pager -o short-monotonic \
  --grep='mmc.*(timeout|error)|I/O error|EXT4-fs error|gpu.*(fault|timeout|hang)|panfrost.*(fault|timeout|hang)|Out of memory|Killed process' \
  -n 80
```

It returned `-- No entries --`. Coverage is limited to those expressions in
retained kernel messages from the current boot, with at most 80 matches
returned; it is not proof that every error class or lost journal entry is absent.

## Input, sound, USB and power

- Physical input devices are the PMIC power key and RK817 headphone switch.
  The remaining listed inputs are virtual passthrough devices, not the handheld
  controls. There is no game-controller or force-feedback device.
- Source enables generic input/ADC driver candidates, but the board DTS omits
  the game-button, joystick and vibrator consumers. Asking the user to press
  those controls now would not constitute a useful evdev acceptance test.
- RK817 ALSA playback/capture and headphone detection are registered. The mixer
  currently selects `HP`; Master playback is 100%. No sound was played, no
  capture was started, and no mixer setting was changed. Speaker-amplifier
  control is omitted from the current board DTS.
- USB UDC is `not attached`, current speed unknown, maximum high-speed.
  Extcon reports `DCP=1`, with USB/USB-HOST/SDP/CDP zero: the PHY classifies a
  dedicated charger connection. This neither proves active battery charging
  nor proves that the port is charge-only.
- `/sys/class/power_supply` exposes no supplies. The `rk817-charger` platform
  child exists, but no battery/charger telemetry is available through that API.
  The final live DT check found only `regulators` and `codec` children under the
  PMIC; its `charger` child is absent, and the charger platform device is unbound.
  Battery state, charge current, charging accuracy and safe low-battery behavior
  cannot be verified from these readings.
- The PMIC is bound as RK817 through the DT compatible string. That is not a
  silicon-ID measurement. Regulator summaries and requested/selector-derived
  voltages were recorded; no value was treated as a multimeter reading or used
  to justify changing a rail. The backlight's physical feed remains unresolved.

## Source explanation for missing battery telemetry

The final live kernel link resolves to the supplied native Linux 6.12.63 Image.
Its dev output's actual `.config` has `CONFIG_CHARGER_RK817=y`, and its
`System.map` includes `rk817_charger_probe`. The driver is not simply omitted
from the compiled kernel.

In exact Linux 6.12.63, `drivers/mfd/rk8xx-core.c:123–128,751–786` creates the
RK817 charger platform child. `drivers/power/supply/rk817_charger.c:1050–1064`
then requires a PMIC DT child named `charger`. Without it, probe returns
`-ENODEV` before registering power supplies, without an error log at that branch.
This matches the observed missing live child, unbound platform device and empty
power_supply class.

The existing binding in `Documentation/devicetree/bindings/mfd/rockchip,rk817.yaml`
requires a monitored battery and sense/sleep-current properties. Driver lines
1084–1152 and `drivers/power/supply/power_supply_core.c:588–625` further require
valid battery characteristics; a vendor `rk817,battery` sibling does not satisfy
that route. Before property validation the driver reads voltage-calibration
registers and computes coefficients (`rk817_bat_calib_vol`, lines 170–189);
that early function does not write registers. An empty child still cannot
provide telemetry, and full initialization later writes gauge state. Do not
guess battery characteristics, sense-resistor values, charge limits or an OCV
table to force a successful probe.

The compiled kernel also contains the generic GPIO/ADC key, ADC joystick,
RK817 codec, Rockchip I2S/thermal and cpufreq support discussed in the source
audit. Missing board consumers are distinct from absent compiled drivers.
Custom ODROIDGO/RGB20S/XU10 symbols in the seed configuration are absent from the
actual compiled configuration; they must not be advertised as shipped drivers.

## Remaining gates

1. Obtain a cooler baseline before load testing; do not silently stop the UI
   after promising to leave it unchanged.
2. Investigate intermittent Wi-Fi loss further. The shorter host/router control
   was clean; do not blame RK915 or change radio power policy from this alone.
3. Battery support needs the existing driver's actual binding requirements and
   validated board/battery data, not guessed fields or transplanted vendor
   charge limits.
4. Button/stick and rumble bindings need source-grounded integration before a
   physical mapping session. Axis labels in the vendor DTS are declarations,
   not measured physical ADC wiring, direction, range or calibration.
5. Audible output, jack changes, USB cable changes, display observations and
   power-cycle/suspend recovery still require the owner. No such tests were run.

## Evidence

Private raw logs are under `~/.local/share/korri/rk3326-stock-shell/logs/`:

- `browser-loop-20260914-190113.log`: read-only survey.
- `browser-loop-20260914-190336.log`: passive thermal/memory and DRM clients.
- `browser-loop-20260914-190635.log`: cgroup accounting, radio/MMC and ALSA.
- `hardware-wifi-proc_6af6.log`: complete 600-probe network result.
- `network-control-20260914-191048-192.168.1.195.log` and
  `network-control-20260914-191048-192.168.1.1.log`: concurrent short control.
- `browser-loop-20260914-191539.log`: live DT, kernel link, unbound charger,
  final thermal/service state and ICMP rule. SoC/GPU still reported 83.8/83.5°C;
  zero browser restarts and zero OOM kills.

Source audit: board DTS and seed configuration under `dts/`, selected
`dts/kernel-trimmed.nix`, and the saved stock `artifacts/rg42t.dts`. The seed
configuration is not substituted for evidence from the actual built kernel.
Exact source archive: `/nix/store/adddx8796jmn91lvgnsd05ycl180as99-linux-6.12.63.tar.xz`.
Compiled configuration: native dev output
`nf4sh4lm7jvkqbshv0xj8kb1bmzc85h5`, `lib/modules/6.12.63/build/.config`.
No credentials or raw owner configuration were included in this report.


## Stage 2: approved browser pause, dock connection and bounded tests

The owner approved pausing Chromium and said the handheld had been moved from
a charger to a laptop dock. The device remained on the same boot and runtime
system. Only the browser service was stopped; compositor, korrid and SSH stayed
active. Browser shutdown was intentional, not a crash or rollback. No automatic
restart was scheduled while hardware tests continued.

### Cooldown

After the cable move, immediately before stopping Chromium, the SoC/GPU sensors
reported 85.384/84.230°C. About 10 seconds after the stop they reported
72.500/73.333°C, with both cooling states zero. At roughly two minutes they were
64.583/65.769°C. The GPU clock settled at 200 MHz, while the unchanged performance
CPU governor selected 1296 MHz. Later test preflights were around 56–58°C.

This is strong evidence that the browser workload contributed substantially
to the heat. It is not a measurement of battery/enclosure temperature or a
fully isolated comparison of the earlier charger and dock power conditions.
No governor, voltage or charger property was changed.

### USB data and console

The dock exposed a Korri/R36T Max composite device at USB high speed (480 Mbit/s
bus signaling). The host bound `cdc_ncm`, showed `10.42.2.24/24`, and exposed
`ttyACM0`. On the handheld, UDC state became `configured`, with `USB=1`, `SDP=1`
and `DCP=0`. This verifies a working USB data connection with this dock/cable;
it does not accept every cable/port or USB host/OTG mode.

Strict host-key-pinned SSH to the existing gadget address `10.42.2.1` returned
the same boot ID as Wi-Fi. This establishes a working Linux-stage wired
management path independent of the WLAN address, not early-boot UART recovery.

The ACM endpoint was verified to share the NCM device's physical USB parent.
A bounded read received 830 bytes containing a recognized NixOS/hostname/login
marker. No newline, command or credential was sent, and raw console text was
not retained. The host's existing administrator access was used only to open
the root/dialout tty; its termios settings were restored. This accepts console
output, not an interactive command session or pre-kernel logging.

### CPU and storage checks

All added workload tests required a cool preflight, used independent worker
runtime limits, and had a 70°C temperature stop. The measurements below did not
hit that stop.

- One SHA-256 worker read `/dev/zero` for 30 seconds. The hottest preflight
  sensor was 58.181°C; highest sampled value under load was 62.500°C. CPU
  frequency remained 1296 MHz in the samples, and OOM kills stayed zero. This
  is a short single-core check, not all-core or GPU stress acceptance.
- A root-owned scratch directory on the verified SD-backed `/var/tmp` held a
  new 32 MiB random source and a 32 MiB copy. Source creation was fsynced; the
  copy used direct reads/writes and fsync; its direct-read SHA-256 matched the
  original source hash. Both files and the directory were removed afterward.
  Highest sampled temperature was 58.181°C.
- `dd` reported 25.2 MB/s for random source creation, 29.2 MB/s for direct
  file-to-file copy, and 21.6 MB/s for direct read into the hash pipeline. These
  small composite operations include generation/hash/filesystem overhead and
  are not isolated SD bandwidth benchmarks or full-capacity integrity tests.
- The first storage attempt used an invalid `dd` flag and stopped before writing
  the source. Correcting `oflag=excl` to the documented `conv=excl` allowed the
  retry above. That first failure was a diagnostic-script error, not a card fault.

### Bounded encrypted payload transfers

An 8 MiB payload was verified in each direction over both Wi-Fi and USB, with
local temperature monitoring and independently timed workers. Upload hashes
matched the host-generated data; downloads had the expected length and hash.

| Link/direction | Elapsed time | Effective payload rate |
|---|---:|---:|
| Wi-Fi upload | 3.919 s | 17.124 Mbit/s |
| Wi-Fi download | 5.968 s | 11.245 Mbit/s |
| USB upload | 1.866 s | 35.965 Mbit/s |
| USB download | 1.686 s | 39.811 Mbit/s |

Elapsed times include SSH setup, encryption, hashing and the monitor's polling
interval. They are application-test results, not radio/USB link-speed claims.
The earlier intermittent ICMP loss is not erased by these successful transfers.

### Longer connectivity check

With Chromium paused and the dock connected, concurrent probes ran at one per
second to Wi-Fi, USB and the router. Each path returned all 600 replies.

| Path | Loss | Mean RTT | Maximum RTT |
|---|---:|---:|---:|
| Wi-Fi | 0/600 | 6.211 ms | 143.247 ms |
| USB | 0/600 | 2.195 ms | 2.333 ms |
| Router | 0/600 | 0.505 ms | 0.819 ms |

This is a clean roughly ten-minute run, not a fix for or explanation of the
original loss. Browser state, power connection and probe rate differ from the
first test. Inspection-tool copying started only after this run ended.

### GLES context and video capabilities

A native query as the existing gameplay user created a Wayland GLES context
using the existing system driver. It returned EGL 1.5, renderer
`Mali-G31 (Panfrost)`, and `OpenGL ES 3.1 Mesa 25.3.2`. This verifies context
creation on the hardware-rendering path, not sustained drawing performance or
which decoder Chromium would select during video playback.

The actual Hantro V4L2 interfaces reported:

| Device | Accepted input | Produced output |
|---|---|---|
| Encoder `/dev/video0` | YM12, NM12, YUYV, UYVY | JPEG only |
| Decoder `/dev/video1` | H.264 parsed slices, MPEG-2 parsed slices, VP8 frames | NV12 |

The decoder reports H.264/MPEG-2 sizes up to 1920×1088 and VP8 up to 3840×2160.
These are advertised format limits, not successful playback results. No codec
job or video stream was submitted. In particular, this Linux encoder interface
does not expose H.264 encoding. That does not establish the silicon's complete
capabilities under other drivers.

Mesa demos and v4l-utils were downloaded prebuilt on the host, copied over USB,
and recursively signature-verified on the handheld. No target build ran. The
headless v4l-utils variant was not cached and was refused with builds disabled;
the cached standard package was used instead. No GUI utility was launched.
The queried temperatures afterward were 57.727/57.272°C, with zero OOM kills.

### State left for physical checks

At boot uptime 9654.76 seconds, the same runtime and boot remained active.
Chromium was still intentionally stopped. Compositor, korrid and SSH had zero
restarts; die sensors both reported 56.363°C. MemAvailable was 683860 kB, about
668 MiB, with no swap and zero OOM kills. The bounded kernel-error query still
returned no matching entries. Both scratch-directory searches returned empty.

The read-only ALSA control `Headphones Jack` reported off, and playback/capture
PCM nodes were closed. The owner reported no wired headphones available, so
plug/unplug testing is deferred. No playback, capture, volume or routing change
occurred. Game controls
and rumble still need board integration; battery characterization and recovery
cycles remain unresolved. No boot default was changed.

The separate browser-load investigation is captured as
`01M2GS38QWXK9HM7ENCMAW6GXG`. Application changes remain paused.

### Follow-up evidence

In the same private logs directory as Stage 1:

- `browser-loop-20260914-193000.log`: authorized stop, cooldown and dock state.
- `usb-ssh-20260914-193006.log`: pinned USB SSH and matching boot.
- `hardware-usb-r36tmax-bounded-cpu-check-20260914-195039.log`.
- `hardware-usb-r36tmax-bounded-storage-check-20260914-195252.log`: rejected flag.
- `hardware-usb-r36tmax-bounded-storage-check-20260914-195420.log`: corrected test.
- `usb-serial-20260914-195509.log`: projected console observations, no raw text.
- `hardware-network-payload-20260914-195727.log`: transfer results and temperatures.
- `cool-network-soak-20260914-195937-{wifi,usb,router}.log`: 600-probe controls.
- `inspection-tools-copy-20260914-201006.log`: prebuilt copy and signature checks.
- `hardware-usb-r36tmax-graphics-video-capabilities-20260914-201139.log`.
- `hardware-usb-r36tmax-final-hardware-state-20260914-201517.log`: final state,
  actual scratch cleanup, jack baseline and the repeated kernel-error query.
- `host-usb-address-proof-20260914-204603.log`: repeated host driver, address/prefix,
  composite-device and ACM observations. This does not independently prove DHCP.
- `host-prebuilt-inspectors-proc_d1c7.log` and
  `host-prebuilt-inspectors-proc_cef3.log`: original retained host download/refusal
  output and the corresponding commands with local and remote builds disabled.
- `hardware-usb-r36tmax-inspector-signature-proof-20260914-204535.log`: repeated
  read-only verification, recording exact paths/options and explicit exit zero.


## Stage 3: owner-observed brightness control

The owner confirmed readiness and watched the complete sequence. A temporary
native swaybg layer showed solid gray at the existing panel mode. Backlight
brightness changed from 833 to 416, then 208, then back to 833 out of 1666.
The driver reported each requested value. The owner confirmed that gray
appeared, dimmed twice, brightened again and disappeared as expected.

An independent 40-second restoration timer was armed before changes. The gray
layer also had its own runtime limit. Normal cleanup verified brightness 833,
stopped the temporary layer and cancelled the unused restoration timer. No
panel-power, mode, governor, voltage or boot configuration was changed.
Chromium remained paused. The fallback timer was armed but did not need to fire;
its failure-recovery behavior was not tested.

This physically verifies dimming and restoration at these three levels. It
does not verify full brightness range, minimum/maximum output, PWM flicker
measurement, DPMS blank/wake, suspend/resume or panel reinitialization.

Evidence: `hardware-usb-r36tmax-screen-brightness-check-20260914-205442.log`
in the same private log directory, plus the owner's `expected` response to the
brightness-observation question. A later read-only check,
`hardware-usb-r36tmax-display-restoration-state-20260914-205834.log`, verified
brightness/actual 833, panel power unchanged at zero, no remaining test units,
and journaled gray-layer stop and timer cancellation about 26.6 seconds after
arming. The gray helper used 95 ms CPU and peaked at 3.2 MiB memory.
The boot ID remained `d34efbf7-915d-4b39-9795-1bb2083b13d5`.
