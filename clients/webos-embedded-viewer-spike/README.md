# webOS experiment 2: local NDL video while Zone draws

**Correction:** the SMP artifact at `b1563de27` linked against SDK APIs absent on
this B7. The parent observed module-load failure at `jboolean_true`, before any
pipeline opened. Its audit also found missing `jboolean_false` and two Starfish
methods. Matching SONAMEs and version floors alone did not prove loadability.

Owner decision: use only `ndl-webos4` from the same pinned ss4s revision
`9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2`. No SMP fallback, audio, common-c,
network client, identity, SDL, power/screensaver hook or app registration.

## Workstation build

```sh
CARGO_BUILD_JOBS=2 \
KORRI_WEBOS_TV_LIBS="$HOME/.cache/research/korri-webos-m2/evidence/tvlibs" \
  nix run --impure .#webos-embedded-viewer-spike-build --max-jobs 1 --cores 2 -L
```

`--impure` permits reading that explicit operator path during evaluation. Nix
then captures the original TV copies as an immutable, content-addressed build
input. They are **not bundled or committed**. A missing evidence path, missing
direct NEEDED provider, or unresolved required packaged import fails the build. No ARM
instruction runs on the workstation, and no task contacts the TV.

The artifact layout is unchanged:

| Path | Purpose |
| --- | --- |
| `bin/run-media-test` | Same app-jail wrapper, argument bounded to 1–25 seconds |
| `bin/media-test` | Video-only CLI; default compiled driver is `ndl-webos4` |
| `lib/libss4s.so` | Pinned ss4s core with the tested close-lifecycle fix |
| `lib/ss4s-ndl-webos4.so` | Pinned upstream NDL webOS 4 backend and utilities |
| `share/korri-webos-viewer-spike/test-1080p30.h264` | Same 300-frame, 1080p30 Constrained Baseline Annex-B fixture |
| `share/korri-webos-viewer-spike/elf-report.txt` | Actual ELF ABI, version and NEEDED results |
| `share/korri-webos-viewer-spike/symbol-report.txt` | Import counts and original TV provider SHA-256s |
| `SHA256SUMS` | Relative SHA-256s for every other packaged file |

Copy the output's contents into `media-test/` beneath the installed app. Replace
that **isolated test directory**, not the app's existing launcher, SDL, bin/lib
directories or registration. Do not retain an older test backend in that test
directory. The parent owns copying and every TV operation.

## TV run and stop

Keep the same Zone process/window running. After entering the existing app jail
and doing the existing supplementary-group/gid/uid drop, preserve the app's HOME
and run this exact command as its non-root UID:

```sh
"$HOME/media-test/bin/run-media-test" 10 2>/tmp/korri-media-test.log
```

The wrapper execs the CLI, so the parent's retained child handle still owns the
exact process for SIGTERM and reaping. It sets `APPID=org.korri.shell` and
`LD_LIBRARY_PATH=<isolated test directory>/lib`, and removes the shell credential
and korrid address/origin environment variables. No RPATH/RUNPATH is used. The
private library path supplies the bundled core and ss4s's explicit
`dlopen("ss4s-ndl-webos4.so", RTLD_NOW)` during check/init. The CLI refuses root
and a different APPID.

The CLI validates the bounded local fixture before module loading. It opens only
video, feeds whole AUD-delimited frames at absolute CLOCK_MONOTONIC 30 fps
deadlines, and stops on a non-OK feed, backend warning/error, or more than 250 ms
lateness. It repeats from the first IDR for runs longer than ten seconds; it never
reloads or selects another backend. Monotonic stderr logs mark module load,
open, first accepted feed and close. Accepted feed is **not** proof of a picture.

SIGINT/SIGTERM/SIGHUP handlers only set a flag. The owning thread closes video,
destroys the player and quits NDL. The argument maximum is 25 seconds; a 25-second
SIGALRM also requests owning-thread cleanup from module-init onward. A blocked
vendor call can still delay cleanup: the alarm is not a forced-kill mechanism.
The parent must monitor/reap its exact child and treat a missing close completion
as failure, not resource release. Normal completion exits 0, failure 1, invalid
arguments/UID 2, handled interruption 128+signal.

Stop immediately for resource conflict, any change to the unrelated HDMI_INPUT 3
pipeline, lost Zone drawing/TV response, backend warning/error or uncertain
cleanup. Send SIGTERM to the owned child; await exit and `close complete`. Verify
the test resource disappeared and the same Zone PID and original HDMI pipeline
remain healthy. Do not stop another pipeline or change power, services,
screensaver, permissions or registration to resolve a failure.

Success requires a physical observer seeing the changing frame counter while
the same Zone process draws/responds, followed by clean resource release. A
compositor screenshot may show only Zone because video uses a separate hardware
plane. Opaque Zone can hide video; accepted feeds then leave visible playback
unproven. Do not alter Zone's design for this experiment.

## NDL executable-memory helper: explicit owner decision

The backend includes and calls upstream `SS4S_webOS_M3_KADP_Fix` **unchanged**.
The parent verified machineName `m16p` and no `MS_VDEC_Init` export in the TV's
`libkadaptor.so`. The helper's `dlsym(NULL, "MS_VDEC_Init")` therefore returns
without patching on this observed TV. Its code still contains mprotect and an
instruction patch; machineName alone does not guard it. This approval is scoped
to the verified B7, not another TV or changed vendor runtime. There is no newly
added dlopen of libkadaptor. Upstream's optional max-resolution helper can dlopen
`libmedia-resource-calculator.so.1`; the CLI does not query capabilities and does
not invoke that optional path. Missing optional resolution support is not a
backend fallback.

## ABI and symbol checks

Every packaged ELF must be ARM32 soft-float EABI5, GLIBC ≤ 2.19, GLIBCXX ≤ 3.4.19,
CXXABI ≤ 1.3.7 and without RPATH/RUNPATH. The executable interpreter must be
`/lib/ld-linux.so.3`. Only `libNDL_directmedia.so.1` and normal libc/POSIX SONAMEs
are allowed besides the bundled ss4s core. The build now checks **actual exported
symbols**, not just SONAMEs: every required packaged undefined dynamic symbol,
including exact versioned imports, must resolve against the bundle or direct TV
providers. Weak undefined symbols remain optional, as required by ELF semantics. Original ARM IFUNC exports are included. Other installed apps' private
ss4s modules are never providers. Vendor libraries' own transitive imports are
not audited as packaged ELFs; this is not a full TV-system closure audit.

GCC's optional profiling/TM hooks remain weak undefined, exactly as emitted by
the toolchain. There are no defsym overrides or substitute hook objects. The
previous NDL build incorrectly bound them to absolute zero; the parent's TV test
found glibc 2.19 treats that as undefined and refuses to load. The audit now fails
if **any** packaged ELF exports a GLOBAL/WEAK ABS symbol with value zero, and
accepts WEAK UND references as optional. No vendor library is bundled.

The same close fix remains: release the player mutex across backend close and
keep frame statistics alive until callbacks stop. Native tests use the actual
ss4s reference sink with a final close-time frame report. They cover validation,
pacing, looping and signal cleanup. Artifact/audit tests use real ARM ELFs,
corrupted copies and original TV evidence. No full Rust suite is relevant.

The fixture remains 4,126,185 bytes. Nix generates it with two encoder threads;
ffprobe decodes/counts all 300 pictures and checks 0.033333 s picture durations.
Its raw `r_frame_rate=60/1` is the field-clock estimate, not picture cadence.
The local test package includes ss4s's LGPLv3 notice and patch, not a complete
publication/compliance bundle. Later common-c work still needs bundled/static
crypto: the TV's libcrypto 1.0.0 does not satisfy common-c's `.so.3` dependency.
