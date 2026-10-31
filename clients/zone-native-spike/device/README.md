# RG35XX SP native-window proof

Throwaway device experiment, 2026-10-30. Only this directory belongs to the
device pipeline; the Zone port is a separate experiment above it.

**Verified:** a bundled Rust + Slint software-rendered window ran fullscreen
from `/tmp`, as `korri-portal`, without installing anything. The kiosk is
restored to Pico and stayed up for 60 seconds after recovery.

| Capture | Median PSS (KiB) | Max PSS (KiB) | CPU (% of one core) |
| --- | ---: | ---: | ---: |
| GAMES, window 1 (24 s) | 447028 | 448812 | 82.032 |
| GAMES, window 2 (24 s) | 442515 | 450528 | 83.046 |
| GAMES, window 3 (24 s) | 448323 | 450977 | 82.766 |
| Attract (60 s) | 446618.5 | 451110 | 134.104 |
| Minimal native hello (24 s; **not the Zone port**) | 12849 | 12849 | 4.148 |

GAMES median across the three window medians: **447028 KiB**. Median CPU:
**82.766%**. `evidence/baseline-summary.txt` includes every MemAvailable and
elapsed-time value. Workloads differ: hello is not a native Zone benchmark.

**Cost/limit:** Chromium sometimes SIGSEGVs when restarted against the existing
Sway session. Restarting the unchanged compositor restored it. The underlying
cause is unknown. This is a working window proof, not a safe product switcher.

## Safety and access

Use `ssh -p 2222 root@10.42.5.1` and `scp -P 2222`. Port 22 does not answer.
The SP answered; the Pro was not used. Read `nix/devices/h700/README.md` and
`nix/device-cache/README.md` before repeating this experiment.

All builds run on the workstation/builders, never the handheld. The handheld
runs an unpacked `/tmp` bundle. No Nix installation, trust changes, partition
writes, system-generation changes, plugin changes, or permission changes are
part of this procedure. A reboot and a compositor restart were needed during
recovery; both used the original system generation.

`evidence/device-before.txt` records the generation, kiosk identity/cgroup,
sockets, initial surface, available memory, and screenshot tool.

## Build and bundle (workstation)

From the repository root:

```sh
nix build --impure --no-link --print-out-paths --expr \
  '(import <nixpkgs> { system = "aarch64-linux"; }).callPackage ./clients/zone-native-spike/device/hello/package.nix {}'

clients/zone-native-spike/device/bundle.sh /nix/store/3ijn8xinr1hspl685fa6d92dmqvs1xvn-zone-spike-hello-0.0.0 zone-spike-hello /tmp/zone-spike-hello-xkb
scp -P 2222 /tmp/zone-spike-hello-xkb.tar.gz root@10.42.5.1:/tmp/
```

The checked lockfile pins Slint 1.18.1 and Fontique 0.11.1. The build uses
`rustPlatform.buildRustPackage` and `cargoLock.lockFile`. It does not modify
`flake.nix`. The observed workstation `<nixpkgs>` resolves to
`/nix/store/2dq7fygjqxv4k5173kr1b6vimi38jjsb-source` (26.05pre-git).
The configured aarch64 builders are fuji and yari.

`bundle.sh PACKAGE BINARY [DIRECTORY]` refuses an existing directory/archive
and non-aarch64 executables. It walks the package's Nix runtime closure,
resolves ELF `DT_NEEDED` recursively with `readelf`, and includes its ELF
interpreter. Each caller's ELF RPATH/RUNPATH resolves its libraries first;
ambiguous closure-wide fallback fails instead of choosing arbitrarily. This
matters because the GCC closure contains two different libgcc_s files.
`libraries.tsv` records each copied library's source.

The extra runtime list comes from the actual Cargo dependency sources:

| Consumer | Library |
| --- | --- |
| wayland-sys 0.31.11 `src/client.rs` | `libwayland-client.so.0` |
| wayland-sys 0.31.11 `src/cursor.rs` | `libwayland-cursor.so.0` (included even if Rust cursor path is used) |
| xkbcommon-dl 0.4.2 `src/lib.rs` | `libxkbcommon.so.0` |
| Fontique's `fontconfig-dlopen` / yeslogic-fontconfig-sys | `libfontconfig.so.1` |

`hello/package.nix` adds these library directories to the output RPATH so
Nix keeps the dlopen dependencies in its closure. A different app must likewise
retain its runtime dependencies. `BUNDLE_DLOPEN='space separated sonames'`
can replace the list for other backends; `BUNDLE_DLOPEN=` bundles only ELF
requirements. This empty override was tested with a prebuilt aarch64 wtype.
This is a Wayland/software bundle, not a universal GPU, X11, or Qt packager.

The bundle also copies xkeyboard-config to `share/xkb` and sets
`XKB_CONFIG_ROOT` in `run.sh`. This is required: the first device run failed
with `XKBNotFound` because libxkbcommon's embedded Nix data path was absent.
No device package was installed to repair it. The launcher uses only shell
builtins before the bundled loader, so even an empty PATH works.

The final bundle contains 17 ELF libraries plus the executable and XKB data:
28 MiB unpacked, 11,032,059 bytes compressed. The release build on fuji took
4 min 54 s after dependencies were available. The first cold dependency
vendoring took additional time; this is not a sub-minute cold-build claim.

The font is embedded Inter from `clients/slint-spike/ui/fonts/`, with its OFL
license. The green background, text, and animated square reuse that spike's
brand tokens. This is not a Zone design proposal.

## Run and stop (device)

The kiosk uses a private read-only bind of the Wayland socket. Its user cannot
traverse `/run/user/1000`, by design. Do not grant directory access or change
ACLs. A transient native service can use the same socket inode and the same
user through its own private bind. Its runtime directory substitutes for the
kiosk's `/run/korri-portal`, which systemd removes when the kiosk stops.

```sh
tar xf /tmp/zone-spike-hello-xkb.tar.gz -C /tmp
cd /tmp/zone-spike-hello-xkb
./lib/ld-linux-aarch64.so.1 --inhibit-cache --library-path ./lib --list ./bin/zone-spike-hello

systemctl stop korri-chromium-kiosk.service
systemd-run --unit=zone-spike-native --collect \
  -p User=korri-portal -p Group=korri-portal \
  -p RuntimeDirectory=zone-spike-native -p RuntimeDirectoryMode=0700 \
  -p BindReadOnlyPaths=/run/user/1000/korri-wayland:/run/zone-spike-native/korri-wayland \
  -p NoNewPrivileges=yes -p ProtectHome=yes -p ProtectSystem=strict \
  -p LimitCORE=0 -p UMask=0077 \
  --setenv=XDG_RUNTIME_DIR=/run/zone-spike-native \
  --setenv=WAYLAND_DISPLAY=korri-wayland \
  --setenv=SLINT_BACKEND=winit-software \
  --setenv=ZONE_SPIKE_FULLSCREEN=1 \
  /tmp/zone-spike-hello-xkb/run.sh
sleep 10
XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 \
  /nix/store/8svfyws68lbybhhf1466wsakqnk74bxz-grim-1.5.0/bin/grim /tmp/zone-spike-native.png

systemctl stop zone-spike-native.service
systemctl start korri-chromium-kiosk.service
```

Verified: the window ran as `korri-portal`, not root. Screenshot:
`evidence/native-hello.png`. `evidence/native-run.txt` records its live PID
2198, user, and cgroup after 10 seconds. Fontconfig configuration warnings and
a missing optional desktop-settings bus did not stop rendering.

Use a shell trap to restore the kiosk if an intermediate command fails.
Do not disable or mask it. If Chromium enters a SIGSEGV restart loop, stop the
native app and kiosk, restart the unchanged compositor, then start the kiosk:

```sh
systemctl stop zone-spike-native.service 2>/dev/null || :
systemctl stop korri-chromium-kiosk.service
systemctl restart korri-compositor.service
systemctl start korri-chromium-kiosk.service
```

This interrupts every Wayland client; it is a recovery cost, not a product
solution. After this recovery the shell PID 9065 and Chromium PID 9085 both
survived 60 seconds with `NRestarts=0`. See `evidence/restored-60s.txt` and
`evidence/restored.png`. The complete change/undo inventory is
`evidence/restoration-inventory.txt`; `evidence/cleanup.txt` verifies all
experiment `/tmp` files and transient units were removed. If the output has powered off, use the existing
sway IPC `output '*' power on`; do not alter the display's policy.

Copy a screenshot back with:

```sh
scp -P 2222 root@10.42.5.1:/tmp/zone-spike-native.png \
  clients/zone-native-spike/device/evidence/native-hello.png
```

## Measurement

```sh
scp -P 2222 clients/zone-native-spike/device/measure.sh root@10.42.5.1:/tmp/zone-spike-measure.sh
ssh -p 2222 root@10.42.5.1 \
  'sh /tmp/zone-spike-measure.sh /system.slice/korri-chromium-kiosk.service /tmp/kiosk.csv 60'
```

`measure.sh CGROUP [RAW.csv] [SECONDS]` defaults to 60 samples. It reads every
process in the cgroup and descendant cgroups, sums PSS from `smaps_rollup`,
and reports the median/max in KiB. CPU is the cgroup `usage_usec` delta divided
by monotonic elapsed time, as a percentage of **one core**. More than 100% is
valid. Both MemAvailable readings are in KiB. It schedules samples against
`/proc/uptime`, so smaps reading cost does not accumulate into the interval.
The final read adds about 0.4 s to the duration on this device. A disappearing
PID is skipped; a still-live unreadable PID fails. Root reads the samples,
but the measured app need not be root.

For a native service use its `systemctl show -p ControlGroup --value UNIT`.
For a `systemd-run --scope` app pass the scope's cgroup the same way. A real
3-second capture of an 8-second sleep scope verified this path, including the
odd-sample median (`evidence/scope-measure-test.*`). Never
use only the browser parent PID: Chromium's renderer/utility processes count.

### GAMES baseline constraint

Unmodified Zone enters attract mode after **30 seconds** without input
(`surfaces/zone/src/state/navigation.ts`, `ZONE_IDLE_AFTER_MS=30_000`). Thus
60 seconds of uninterrupted stationary GAMES is not possible. No timer or
portal-code override was made. The mixed 60 s trace is retained as a labelled
note, not an all-GAMES baseline. A pure-attract trace is separate.

The supervisor requested three shorter GAMES windows instead. The windows
are **24 seconds**, each after **5 seconds** with no input. This preserves the
user's minimum settling time and finishes before the 30 s idle transition,
including sampling overhead. A 25 s window after that settling time would
cross the timer. Screenshots at each window's end verify the actual page.
The look is **Blend**, the default without a `zone.look` preference. No look
write was made. Modern was not reached through virtual-keyboard navigation.

The surface selector is grounded in
`clients/portal/src/surface/surface-preference.ts`: `?surface=zone` selects
Zone and remembers `korri.surface`. During measurement a single runtime-only
systemd drop-in overrides `KORRI_WEB_SURFACE_URL` to this URL. Removing the
drop-in and restarting with the original `?surface=pico` also restores that
remembered preference. No persistent unit was edited.

## Checks

```sh
nix shell nixpkgs#shellcheck nixpkgs#nixfmt nixpkgs#rustfmt -c sh -c '
  shellcheck clients/zone-native-spike/device/*.sh &&
  nixfmt --check clients/zone-native-spike/device/hello/package.nix &&
  rustfmt --check clients/zone-native-spike/device/hello/src/main.rs &&
  sh clients/zone-native-spike/device/test.sh'
```

The device bundle loader's `--list`, native screenshot, two inspected motion
frames, raw native/kiosk cgroup measurements, and a real scope capture are the
integration checks. Independent Python checks recomputed sample counts,
medians, and maxima from the final CSVs; all matched. Final kiosk captures
sampled 13 processes and retained PID 2510 with no automatic restarts. No full repository suite is needed
for this standalone device experiment.
