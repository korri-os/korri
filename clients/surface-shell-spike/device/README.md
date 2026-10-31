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
  /tmp/surface-shell-sp.tar.gz _test/a,_test/b /tmp/shell-results
# Later, with real packages in the bundle:
# device/drive.sh /tmp/surface-shell-real.tar.gz zone,pico /tmp/shell-real-results
```

Use a fresh output directory/archive name for each bundle and result directory.
`bundle.sh` retains the proven ELF loader/library/XKB collection from
`zone-native-spike/device/bundle.sh`. It adds `bin/surfaces/` and `bin/fixtures/`
next to the executable. Set `SHELL_BUNDLE_ROOT` to use packages from a different
checkout. `SHELL_ROOT` overrides the executable-relative package lookup at run
time. The companion `wtype` is only for deterministic F2/arrow automation.

`drive.sh` copies over SSH port **2222**, runs the script, and collects evidence.
Default host: `root@10.42.5.1`; `SHELL_DEVICE` overrides it. The runner refuses
live processes in `korri-games.slice` or unknown Sway windows, and does not stop
a game. It powers the existing Sway output on, stops only the kiosk at runtime,
starts the full-screen shell, sends Right/Left and three F2 presses, captures
with the existing `grim`, then stops the shell and restores the kiosk.

Results: `surface-start.png`, `surface-right.png`, `surface-left.png`,
`switch-{1,2,3}.png`, the Sway tree, loader list, service identity, journal with
`SHELL_STATS` compile/logic-load/first-frame times, and `kiosk-after.png`.
Every device run has an EXIT/HUP/INT/TERM recovery trap. Evidence and bundles
remain in `/tmp` so a failed run can be inspected. Each run uses a fresh `/tmp/surface-shell-results-<time>-<pid>` directory.
Use a fresh bundle name for each successful extraction. The driver checks
locally that the archive contains every requested package before it copies.
An absent `korri-games.slice` cgroup means no processes in that slice; unknown
Sway clients still block the run.

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
