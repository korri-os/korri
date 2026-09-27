# Mini V2 controller iteration

Run a browser/controller test on the approved spare SD without rewriting the
image. Build on the workstation or its configured ARM builder. The handheld
receives only prebuilt executables through authenticated SSH.

## Commands

Run from the checkout that contains the candidate code:

```sh
nix run .#rpminiv2-input-iterate-check
nix run .#rpminiv2-input-iterate -- rpmini \
  --ssh-config "$SSH_CONFIG" --expected-system "$CURRENT_SYSTEM"
```

`rpmini` is an operator-defined SSH alias. The SSH configuration must pin the
known device host key. `CURRENT_SYSTEM` is the approved store path observed at
`/run/current-system`, not a proposed generation.

The baseline runs the production kiosk arguments with a test-only launcher.
Add `--isolate-portal` to put readable empty, read-only mounts over other
virtual joystick sysfs parents inside the kiosk's mount namespace. It changes
no device ACL, kernel, inputd target, or other service's view. Unreadable masks
are not equivalent: they can stop udev enumeration, including the portal pad.

`--trace-input` adds a passive input-open/close witness only to the unprivileged
kiosk wrapper. It logs numeric results and literal numeric input-node paths,
not buffers, arbitrary paths or credentials. `--check-rollback` intentionally
interrupts atomic override installation and verifies recovery.

`--isolate-portal --keep-on-pass` leaves only the passing input view active with
the production launcher. No probe or preload library remains active. This
view is deliberately per-boot and must be refreshed after inputd recreates its
targets. The loop recognizes this owned view as its rollback baseline.

## What one run does

1. Build `korri-portal-input-probe` off-device and retain its Nix GC root with
   the evidence. The production `korri-portal-shell` source and package stay
   unchanged. Only this test package applies `clients/linux/diagnostics/shell.patch`.
2. Verify the Mini V2 model, SD root and boot partitions, approved generation,
   inputd readiness, absence of a live game, and unmodified kiosk configuration.
3. Copy the prebuilt launcher and D-pad replayer to RAM. Verify their checksums
   and require all runtime library references to be present already. No Nix
   signature bypass, download, or build runs on the target.
4. Restart only the kiosk with its original browser arguments and the test
   launcher. The existing private CDP pipe reports booleans and bounded counts.
   It never reports credentials, URLs, page text, arbitrary identifiers, or
   exception messages. There is no debugging network port.
5. Wake the display with inputd's existing native idle-notify operation, then
   inject four Right pulses into the verified inputd portal target. The native
   replayer refuses another identity, a held physical button, or a non-neutral
   D-pad. It never grabs a device or emits confirmation or power keys. It
   releases the axis at exit and on an ordinary termination signal. A separate
   read-only joydev handle must observe all four pulses and releases.
6. Capture before/after browser state and a screenshot, then restore the
   original kiosk even when the trial fails. An unverified rollback stops the
   loop rather than permitting another trial.

The result is PASS only when Chromium exposes the standard portal controller,
replay completes, sampled focus/visibility stay valid, animation frames advance,
and native DOM focus changes within the replay window. The gate requires
samples near both boundaries with no gap over one second. Backend DOM node IDs
avoid confusing DOM reordering with focus movement. Sampling cannot rule out
every sub-sample change or concurrent physical input. A service being active
or an input event reaching the kernel is not a pass. No DOM keyboard event or
direct focus call substitutes for gamepad input.

Evidence remains in the printed private `/tmp/rpmini-input-iteration-*`
directory on the workstation. RAM staging on the handheld disappears at reboot.
A screenshot is actual page output, not a generated mockup.

## Limits and operating approval

The user approved autonomous component trials on the spare SD in decision
`f42d47a4-37d0-43ba-a9bc-ca03b039fbda`. This includes service restarts, D-pad-only
replay with no active game, and rollback. It does not authorize an SD rewrite,
kernel replacement, firmware/internal-storage writes, broader device access,
or weakened signature checks. Physical tests still require current readiness.

Replay covers portal-target to browser to Shift focus. It does not test the
physical buttons, InputPlumber's raw-device normalization, or display wake.
Those need separate hardware acceptance. Kernel and full Chromium builds are
not fast component iterations.

The diagnostic isolation uses the current kernel-provided sysfs parents.
Hotplug after setup can change those parents. Do not ship that snapshot as a
permanent policy. A game started outside this test stops rollback rather than
restarting its kiosk unexpectedly.

The existing `clients/portal/nix/deploy.py` uses `--no-check-sigs`. This runner
does not call that path. Repair of that separate deployment task is outside
this controller test.

## Current-image startup correction

The c5bdcff3 image configured a cross-store `swaymsg` symlink that inputd rejects.
Source commit `a9ade057` selects `sway-unwrapped` and adds an emitted-action
regression. The verified on-device one-off correction is now in systemd's
native persistent override directory:

`/etc/systemd/system.control/korri-inputd.service.d/90-swaymsg-canonical.conf`

Its content is the previously tested action environment, changing only the
executable. `systemd-analyze unit-paths` on this image reports that persistent
search path; `/etc/systemd/system` itself is a read-only NixOS symlink. The RAM
copy was removed and inputd restarted successfully using the persistent copy.
This is not a claim of a reboot test.

Remove that one-off override as part of a future approved generation update
that contains the source correction. It must not override a later generation's
executable path after the current generation can be garbage-collected.
