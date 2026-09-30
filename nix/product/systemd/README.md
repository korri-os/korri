# Product systemd: stop thaws directly frozen units

The owner approved this downstream semantic change on 2026-09-26 for the
RP Mini V2, and made it product-wide on 2026-09-30. It is not an upstream
bugfix-only backport. Upstream systemd 258.2's `TEST-38-FREEZER.sh:279`
expects a frozen unit's stop to fail.

## Boundary and implementation

`../nixos-module.nix` imports `module.nix` for every product device. Recovery
configurations that do not import the product keep the original systemd package.
The override appends one native C patch and refuses versions other than 258.2.
It adds no polkit or DBus policy and does not enable kiosk freezing itself.

The source inspected was
`/nix/store/zfx7f4w3pmv21n8f79ly9nq3gcv35vlg-source`:

- `src/core/unit.c`: `unit_stop`, `unit_freezer_complete`, and the public
  `unit_freezer_action` pending-job guard.
- `src/core/cgroup.c`: `unit_cgroup_freezer_action` and cgroup event completion.
- `src/core/service.c:5646`: the unit-type callback pauses/resets the watchdog.
- `src/core/job.c:879`: restart first executes the ordinary stop operation.
- `src/core/dbus-unit.c`: pending freezer reply ownership and cancellation.

Only `unit.c` changes. Stop calls the unit-type thaw callback, not the public
API that rejects pending jobs. An outstanding Freeze invocation receives
`FreezeCancelled`, including when the kernel never finished freezing and thaw
completes synchronously. If thaw is asynchronous, stop returns `-EAGAIN` to
PID1's job engine. Freezer completion requeues only stop/restart jobs. An
existing Thaw invocation keeps its normal completion reply. No caller retries.

The code checks the ancestor slice states before thawing. It does not thaw an
ancestor to make a child stop succeed. A child frozen by a still-frozen slice
still has job result `frozen`; the owner must thaw that slice separately. This
also applies to an independently frozen child inside a frozen slice. Start and
reload retain their existing behavior. Out-of-band cgroup writes retain
upstream's separate state-tracking behavior.

## Verified on 2026-09-26

Run on the development machine, never on a handheld:

```sh
nix build .#checks.x86_64-linux.rpminiv2-systemd-freezer --no-link --print-build-logs
nix build .#nixosConfigurations.rpminiv2.config.systemd.package --no-link --print-out-paths --print-build-logs
```

The final x86 NixOS VM run passed in **100.90 seconds**. It checked
`/proc/1/exe` against the patched package before testing. Root regression calls
use native DBus manager methods and await `JobRemoved`, not a wrapper retry.

| Regression | Verified result |
|---|---|
| First frozen stop and restart | Both jobs `done`; old process wrote its SIGTERM marker; restart changed PID |
| Asynchronous thaw | Fully frozen service resumed and the original stop job completed |
| Outstanding FreezeUnit | FUSE-held kernel read kept `freezing` pending; stop canceled the request, then completed after read release |
| Concurrent freezer operations | 30 freeze/stop and thaw/stop races; every stop completed once and every freezer call returned |
| Pending stop job | External FreezeUnit and ThawUnit both rejected with `UnitBusy` |
| Frozen ancestor | Child stop failed without thawing parent or sibling; independently frozen child survived parent thaw |
| Start and reload | Start stayed a no-op; reload stayed rejected; neither thawed the unit |
| Watchdog | Remained active past WatchdogSec while frozen; frozen restart and stop delivered SIGTERM |
| Korrid crash dependency wiring | SIGKILL of a fixture korrid stopped the frozen kiosk and pulled it back in; one daemon restart |
| Unprivileged stop/restart | Passed with only kiosk-specific `stop`/`restart` polkit rights; external thaw and another unit's stop denied |
| Ordinary poweroff | Frozen service fsynced a SIGTERM marker; marker verified after restarting the VM |
| Package isolation | Check asserts product patched, recovery unpatched; evaluation found no patch in any other exported NixOS configuration |

The crash test extracts `wantedBy`, `requires`, `after`, and `partOf` from the
actual product kiosk, and `Restart`/`RestartSec` from its korrid. Processes are
fixtures, not the Rust daemon, Chromium, nginx, or the compositor.

All **six** upstream `TEST-38-FREEZER.sh` cases ran on the patched PID1:
`dbus_api`, `preserve_state`, `recursive`, `systemctl`, `systemctl_show`, and
`watchdog`. The watchdog case includes daemon reload and reexec. The test
fixture changes only the deliberate stop-failure expectation and removes its
subsequent thaw/duplicate stop because the transient service is now collected.
The six completed cases and `/testok` are required, so a skipped suite fails.
Upstream's original systemctl calls and timing waits are retained; no broad
polkit rights are granted to make those root tests pass.

Evidence:

- VM derivation: `/nix/store/933hbkr27034i27c84p5r4jq1wpvpfn0-vm-test-run-rpminiv2-systemd-freezer-stop.drv`
- VM output: `/nix/store/rgj6a8j23drilsz8npkycm0s7hvv2ikx-vm-test-run-rpminiv2-systemd-freezer-stop`
- Upstream trace: `upstream-freezer.log` in that output.
- Patched x86 PID1 package: `/nix/store/kwlalgjrlcpxkc6a2zyn1f59hflv0j1s-systemd-258.2`
- ARM build log: `nix log /nix/store/h2jq31qnj7aiafwfmsi9s7lx3qrgakg7-systemd-258.2.drv`
- ARM package: `/nix/store/0x2s64hmgqd7d1hqx5g11503fa0am22v-systemd-258.2`
- ARM NAR hash: `sha256-eq00YpeoDgjMHfTXJgdMlFKRnlJUYQEsqm7TAwPjtNw=`

The ARM derivation built on configured builder `ssh-ng://simonwjackson@fuji`
and copied back to the development store. `file` identifies its PID1 as an
AArch64 ELF executable. This is a prebuilt ARM artifact, not an ARM runtime
acceptance result. `git diff --cached --check` and Nix formatting checks passed.

## Remaining limits and cost

- No target device was contacted or changed. No image was rewritten, deployed,
  pushed, or merged. Parent review and deployment approval remain required.
- No full systemd integration suite or handheld kernel test ran. In-flight
  freeze is deterministic; thaw/stop interleavings use race stress rather than
  a separately held kernel thaw. Runtime behavior of real korrid/Chromium is
  not established by their dependency fixtures.
- An uninterruptible process must still leave its kernel wait before it can
  handle SIGTERM. Thaw does not solve stuck I/O or replace normal stop timeouts.
- The native DBus policy already denies an unprivileged Manager.ThawUnit call.
  The stop patch needs no change to that policy. Permissions for a future
  kiosk-freeze caller are a separate integration question.
- A downstream PID1 patch requires review on every systemd update. The ARM
  package exists on Fuji and locally; this work does not publish a signed
  device-download route or authorize activation. Keep the previous generation
  until parent integration verifies prebuilt delivery under the device policy.
