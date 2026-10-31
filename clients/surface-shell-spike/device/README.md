# THROWAWAY runtime surface-shell device proof

This is not an installed client or a deployment pipeline. Read `CONTRACT.md`.
All Rust compilation and Nix builds run on the workstation, fuji or yari.
The SP only runs prebuilt files unpacked under `/tmp`. No signatures, groups,
ACLs, input permissions, persistent units or device generations are changed.

**Root exception:** the transient shell runs as root because the gamepad nodes
belong to root / group `korri`, neither of which `korri-portal` has. This is
allowed only for this spike. It does not solve product input isolation.
The shell never grabs a device. The compositor socket still has a private
read-only bind. The service has `NoNewPrivileges`, `ProtectHome`,
`ProtectSystem=strict`, a private runtime directory and no core dumps.

**Run gate (2026-10-31):** ask the user before stopping the kiosk or launching
the shell on SP. Give the exact `drive.sh` command. Read-only SSH checks are
allowed before approval. Building/bundling on workstation does not need it.

## Build and bundle (workstation)

From the repository root:

```sh
nix build --impure --no-link --print-out-paths --expr \
  '(import <nixpkgs> { system = "aarch64-linux"; }).callPackage ./clients/surface-shell-spike/package.nix {}'
# Save the printed store path as PACKAGE. This runs on fuji / yari, not SP.

# Prebuilt keyboard automation companion, also built on workstation/builders:
nix build --impure --no-link --print-out-paths --expr \
  '(import <nixpkgs> { system = "aarch64-linux"; }).wtype'
BUNDLE_DLOPEN= clients/zone-native-spike/device/bundle.sh \
  WTYPE_STORE_PATH wtype /tmp/shell-wtype
SHELL_WTYPE_BUNDLE=/tmp/shell-wtype \
  clients/surface-shell-spike/device/bundle.sh \
  PACKAGE surface-shell-spike /tmp/surface-shell-sp

clients/surface-shell-spike/device/drive.sh \
  /tmp/surface-shell-sp.tar.gz --surfaces zone,pico \
  --hold-seconds 120 --results /tmp/surface-shell-real-results
```

Use a fresh local output directory/archive name when rebuilding a bundle.
Use a fresh local results directory for each run. Old device extractions are
removed after safety checks, before unpacking the new archive.
`bundle.sh` retains the proven ELF loader/library/XKB collection from
`zone-native-spike/device/bundle.sh`. It adds `bin/surfaces/` and `bin/fixtures/`
next to the executable. Set `SHELL_BUNDLE_ROOT` to use packages from a different
checkout. `SHELL_ROOT` overrides the executable-relative package lookup at run
time. The generated launcher sets `SHELL_ROOT` to its own `bin/` directory,
because explicit ld-linux execution makes `current_exe()` identify the loader.
The companion `wtype` is only for deterministic F2/arrow automation.

`drive.sh` copies over SSH port **2222**, runs the script, and collects evidence.
Default host: `root@10.42.5.1`; `SHELL_DEVICE` overrides it. The runner refuses
live processes in `korri-games.slice` or unknown Sway windows, and does not stop
a game. It powers the existing Sway output on, stops only the kiosk at runtime,
starts the full-screen shell, sends Right/Left, measures the first surface for
24 seconds, switches with F2 and measures the second surface for 24 seconds,
then returns to the first. With the default package list these are Zone then
Pico. The copied `measure.sh` is the native spike's same cgroup PSS/CPU sampler:
PSS includes every process in the cgroup; CPU is percent of one core, not the
whole SoC. It excludes the compositor. Do not press buttons during those two
measurement intervals, or their labels no longer prove the shown surface.

The script then prints `Interactive observation: 120 seconds` and leaves the
shell up so the user can watch and press L1+R1. `--hold-seconds N` sets this
interval (0..3600, default 120). F2 and the gamepad combo continue to work.
Finally the runner stops the shell, restores the kiosk, and checks both the
kiosk and the actual Chromium browser PID for 60 seconds. It restarts the
unchanged compositor and retries recovery if needed. EXIT/HUP/INT/TERM also
restore the kiosk after it has been stopped.

Results include `phase-0.csv/.txt` (Zone), `phase-1.csv/.txt` (Pico), screenshots,
the Sway tree, loader list, service identity, invocation-scoped journal,
`switch-timings.txt`, and `kiosk-stability.txt`. Each run has a unique remote
`/tmp/surface-shell-results-<time>-<pid>` directory. The driver copies all
results locally, then deletes this spike's extracted bundle, archive, runner,
measure companion and `/tmp/surface-shell-results*`, including earlier failed
runs. Cleanup refuses to delete the files of an active shell service. If the
SSH/evidence copy fails, files can remain for diagnosis.

The driver checks locally that every requested package has `main.slint` and
`logic.js` inside the archive. An absent inactive `korri-games.slice` cgroup
counts as empty. Nonempty game slices and unknown Sway clients still block.

### Exact-archive workstation proof

Before requesting a device run, unpack and execute the **same** archive:

```sh
nix shell nixpkgs#qemu-user nixpkgs#proot nixpkgs#xorg.xorgserver \
  nixpkgs#xdotool nixpkgs#imagemagick \
  -c clients/surface-shell-spike/proof-bundle.sh \
  /tmp/surface-shell-sp.tar.gz clients/surface-shell-spike/evidence/bundle
```

On x86_64, PRoot inserts QEMU for foreign ELF execution without changing the
bundle's `run.sh`. The script uses a fresh extraction, caller cwd `/`, an owned
private runtime directory, fullscreen, software renderer, stats and `zone,pico`.
`SHELL_ROOT`, `SHELL_SIZE` and `LD_LIBRARY_PATH` are unset. Only the display edge
differs: X11/Xvfb rather than the SP's private Wayland socket. This checks the
actual aarch64 executable, loader and launch script; it is not an SP performance
measurement or a Wayland proof.

## Manual run and recovery (device)

The default shell window is 640x480; `SHELL_SIZE=1280x720` changes it.
`SHELL_FULLSCREEN=1` is used on the SP. `SHELL_SURFACES=zone,pico` is the default.
`SHELL_STATS=1` logs frames/s every five seconds (actual redraws, not refresh
rate), and each first completed frame. Static test screens can report 0 fps
when no redraw is needed. Switch timings include cold runtime compilation.

The full transient-service command is in `surface-shell-run.sh`. Do not run
Nix or compilation commands on SP. The private Wayland bind avoids opening
`/run/user/1000` to `korri-portal` or changing any permission.

To recover if the runner itself is lost:

```sh
ssh -p 2222 root@10.42.5.1 '
  systemctl stop surface-shell-spike.service 2>/dev/null || :
  systemctl start korri-chromium-kiosk.service
  sleep 20
  systemctl show -p ActiveState -p SubState -p NRestarts korri-chromium-kiosk.service
'
```

If Chromium SIGSEGVs/restarts against the old session, use the existing recovery:

```sh
ssh -p 2222 root@10.42.5.1 '
  systemctl stop surface-shell-spike.service 2>/dev/null || :
  systemctl stop korri-chromium-kiosk.service
  systemctl restart korri-compositor.service
  sleep 3
  systemctl start korri-chromium-kiosk.service
  sleep 20
  systemctl show -p ActiveState -p SubState -p NRestarts korri-chromium-kiosk.service
'
```

Verify active/running, a stable MainPID and NRestarts=0. Take a recovery
screenshot. Restarting the compositor interrupts other Wayland clients; it is
a recovery cost, not a product switching design. Do not disable or mask units.
If the output is asleep, use the existing Sway IPC `output '*' power on`.
