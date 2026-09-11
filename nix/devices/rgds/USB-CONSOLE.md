# USB console startup repair

The first candidate, `b26a379b4b7d`, boots from SD but loses its USB login.
The saved card journals show the same startup race in two boots. In the
latest boot, agetty tries `/dev/ttyGS0` at 15.9 seconds, the gadget becomes
ready at 20.0 seconds, and the login service exits at 25.9 seconds. It never
restarts. That boot later handles the power key and shuts down cleanly.
This evidence does not establish the cause of a separate delayed freeze.

## Permanent source fix

The standalone `serial-getty@ttyGS0.service` masked the base
`serial-getty@.service` template. It retained NixOS's template drop-in with
`ExecStart`, but lost the template's device dependency, terminal setup, and
`Restart=always` policy.

An instance drop-in is equally wrong: systemd keeps only the most specific
drop-in of a given name, so it would discard the packaged agetty command and
root autologin. `sd-image.nix` therefore declares no instance unit at all. A
udev rule tags the gadget tty so systemd starts `serial-getty@ttyGS0.service`
when `/dev/ttyGS0` appears, using the unmodified template and NixOS's own
template drop-in.

`usb-console-check.nix` runs the real NixOS unit-directory generator against
the exported declarations. It rejects both a masking instance file and an
instance drop-in, requires the template's device, terminal, and restart
rules, requires the packaged agetty and root autologin, and requires the udev
rule. Earlier drafts of this fix failed that check.

No kernel, bootloader, WiFi, or eMMC change is part of this repair.

## Owner-approved temporary SD repair

The owner chose a small, reversible SD repair rather than reflashing the
whole filesystem. This installed generation still starts the getty from
`getty.target`, before the gadget exists, so a verbatim copy of the template
would fail its device dependency and never start later. The temporary repair
therefore derives a retrying unit from the installed systemd 258.2
`serial-getty@.service`: it removes the absent-device binding and adds
`StartLimitIntervalSec=0` and `RestartSec=2`. It is written to:

```
/etc/systemd/system.control/serial-getty@ttyGS0.service
```

The systemd unit search path gives `system.control` priority over the
incomplete instance in `/etc/systemd/system`. The existing NixOS template
drop-in still supplies the packaged agetty/login paths and root autologin.
The replacement base file restores the original template's missing rules.
It does not change the Nix store or the selected boot generation.

The derivation source, verified against the exact downloaded candidate, is:

```
/nix/store/xn64fgz1z7jz4nnhln9nkdkd4p1bparg-systemd-258.2/example/systemd/system/serial-getty@.service
```

Its SHA-256 is
`59269f2d99dd786b600cc0e0d2ef178aa5dd4b129780c953fbedc2e4cddd9ee8`.
The operation refuses to replace an existing control file. Its receipt is
saved on the laptop at `~/Downloads/korri-rgds/usb-console-recovery.txt`.
That receipt proves application and read-back, not a working device login.
Applied on 2026-09-11 to disk sequence 27; written file SHA-256
`4222517aba71a68fddf33714f7961131961f3121ef8c4b9548cf2715a73f01aa`.

## Verify and retire

On the next device boot, verify USB login with the cable attached both before
and after startup. Check `systemctl status serial-getty@ttyGS0` and collect
its journal. Confirm `/` uses the SD card before any further deployment.
Then observe an idle interval and a USB disconnect/reconnect. A working
initial login alone does not prove the delayed symptom is resolved.

The control file is temporary local state. After installing a prebuilt
generation containing the permanent fix, confirm that generation has the
instance drop-in and the original serial-getty template. Remove only the
control file named above, reload systemd, and restart that USB getty. This
interrupts the current serial connection; reconnect to verify the new unit.
Do not leave the control file masking future template updates.

Until the corrected generation is installed, deleting that control file
reverts this repair and restores the original broken startup behavior.
Do not remove or modify any Nix-store unit file.
