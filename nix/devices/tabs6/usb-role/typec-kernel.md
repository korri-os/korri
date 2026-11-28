# Kernel-owned Tab S6 USB-C data role and battery source

The Korri product uses `s2mm005-read-only` (Linux 7.3.4). It owns the data
role and, since 2026-11-25, the battery-powered source for non-PD sinks such
as the Razer Kishi V2, the way Samsung's Android kernel does it. The console
kernel stays unchanged. There is no user-space automatic role or boost policy,
and user space never enables boost.

## Ownership and safety

The driver selects dwc3 roles through `usb_role_switch_set_role`. It registers
a standard Type-C port and partner; the power role reports SOURCE only while
our boost is on. No Type-C swap operations are offered.

The **s2mm005 stays strictly read-only**: register-address write plus
repeated-START read, no mailbox commands, no interrupt clear, no firmware,
flash or OTP. IRQ123 is not requested (it can stay asserted without an
acknowledgement). The **only writes** go to the SM5705 charger at 0x49, and
only to the bits Samsung's `sm5705_charger_oper.c` touches, each a
read-modify-write verified by readback (`sm5705_update` refuses any other
register or mask):

| Register | Bits | On (stock USB_OTG table) | Off (stock CHG_ON table) |
| --- | --- | --- | --- |
| FLEDCNTL6 0x20 | 3:0 boost voltage | 0x0B, 5.1 V | 0x05, 4.5 V |
| CHGCNTL6 0x14 | 3:2 OTG current | 3, 1.5 A | 0, 500 mA |
| CNTL 0x0c | 2:0 mode | 7, USB_OTG | 0 (SUSPEND) for 100 ms, then 5, CHG_ON |

Order on enable, as in the former manual helper: voltage, current, a fresh
interlock re-check ("configuration is not permission"), then mode 7 with
readback, then host. Order on revoke: boost off with readback, then the data
role. Every 7 → 5 passes through SUSPEND for 100 ms. Stock does that only for
gauge id < 5 (this tablet reports 0x0b); we keep it always so the boost and
the charger input are never commanded at once. A failed boost-off is retried
(3 tries per poll, every poll) and stops the policy in device role until the
driver is rebound; a failed enable turns boost off and backs off.

Polling: 100 ms with boost off, 20 ms while boost may be on. The SM5705
charger and MUIC IRQ lines are not used: their latches are read-to-clear and
nothing in Linux owns them, but their masks are unknown and writing masks is
out of scope; polling at 20 ms bounds our part of the reaction.

### Sink host (charger supplies the port)

Unchanged. Three consecutive fresh samples permit host only when **all** of:
PD state 21, attached, source=0, DFP=1, MUIC VBUS exactly 01, CNTL mode 5.
Any other sample or a read error selects device at once. A normal PC/UFP never
qualifies. Each poll also compares the decision with the switch's actual role:
an outside write to the role file is logged (`role changed outside policy`)
and undone after 1 s (added after an overlapping deploy wrote it 293 times).

### Battery source (stock grounding, Samsung 37cf30da)

`s2mm005_cc.c` `process_cc_attach` (:748): boost on (`vbus_turn_on_ctrl(1)`)
in SRC states 1, 3, 4, 5, 6, 7 (:904–941); off in SNK states 17, 18, 21 and
ErrorRecovery 29 while host (:958–981), on PR_SWAP 52 (:861–863) and on detach
(:1035–1070, `process_cc_detach` :1120–1132). Stock never turns boost off in
state 14 (`State_PE_SRC_Wait_New_Capabilities`, `s2mm005.h` enum) and never
checks IS_SOURCE or ATTACH_DONE. The OTGFAIL handler (STATUS3 bit 2) turns
OTG off.

Enable, boost off, two consecutive fresh samples with **all** of:

| Interlock | Check |
| --- | --- |
| Source state | 1, 3–7, or 14. **14 is our measured extension of stock**: the unpowered Kishi sits there (0 ↔ 14 every ~15.8 s, `kishi-unpowered-cycle-replug.txt`); stock enters boost earlier and only keeps it in 14 |
| No incoming VBUS | MUIC VBUS 00 and STATUS1 exactly VBUSUVLO (no VBUSPOK, OVP, LIMIT, wireless input) |
| On battery | gauge Discharging (fresh read; the gauge decides only enables) |
| No charger/PC signature | MUIC DEVT1 none, UART (measured) or OTG; DEVT2 zero; DEVT3 zero except bit 2 DCD_OUT_SDP, which stays latched after the charger is unplugged (measured 0x04 with VBUS 00, STATUS1 0x22, Discharging; `kishi-battery-unplug-devt3.txt`) |
| No fault | STATUS3 OTGFAIL clear |
| Boost off | CNTL mode 5 |
| Not backing off | see below |

Revoke at once, every poll while boosting: read error (any failed status
read; a failed dummy read of 0x0008 is logged and ignored, as stock); CNTL no longer mode 7
(including the charger forcing mode 5 on incoming VBUS); OTGFAIL, VBUSOVP,
VBUSLIMIT or wireless input; any MUIC charger/PC signature; and any state
outside the source set: 17, 18, 20, 21, 29, 52 or unknown. State 0 is a
detach after its 2.5 s episode (below).

A FUNC_STATE read with flags 0xff (every flag at once: not a plausible state)
is suspect: while boosting it is re-read on the next poll (about 30 ms), and
only a confirmed non-source state revokes; two suspect reads in a row count as
a read error and revoke; every other check stays immediate. Measured once:
state 15 / flags 0xff between two 14s at 2227.98 s revoked boost and
power-cycled the Kishi (owner decision 2026-11-25). A suspect read never
counts toward an enable.

`boost_telemetry` (module parameter, off by default, writable in
`/sys/module/s2mm005/parameters/`) logs every poll while boosting: STATUS1–4,
MUIC INT1–3 (reading clears these latches; nothing else in Linux uses them)
and the gauge current. It exists to find a charger-arrival signal that works
while we source VBUS.

**Fixed grace window, 1.5 s (owner decision 2026-11-25).** After a verified
enable the chip re-detects: state 0 for about 0.7 s (attached flickering),
then 14 with attached 0 (`phase-b-host-transition.txt`, 95–794 ms). During
the window only 0 and 14 are tolerated, only while CNTL reads 7; everything
else above still revokes at once. The window never extends. **State-0 episodes, 2.5 s each (owner decision 2026-11-25).** Even while we
boost, the chip keeps its ~16 s re-detection cycle and drops to state 0
before 14 returns (`kishi-battery-boost-cycle.txt`). Measured with boost on:
0.71 s right after enable, then 1.35–1.39 s per cycle (unpowered episodes were
~0.85 s). While boosting, a state-0 episode is tolerated for at most 2.5 s
from its first poll, never extended; a source state ends it; longer is a
detach. Removing the Kishi on battery therefore leaves boost on for at most
~2.5 s into an open port. A start that is revoked in its window or still reads 0 when it ends is
a failed start: no
re-enable until the chip shows a source state for two fresh polls, after a
backoff of 3, 6, 12 … 60 s (reset by 60 s of stable boost). Any other revoke
waits a fixed 3 s before the next enable. Only enables wait.

After a revoke caused by a charger, sink host requalifies from zero: DFP
arrives about 1.9 s after SNK_Ready (measured), so the Kishi re-enumerates
rather than keeping host across the hand-over.

**Limitation: a charger plugged into the Kishi while we boost is not
detected (measured 2026-11-27, build 394qyl2m).** Simon plugged a charger
into the Kishi pass-through at about 1300 s while boost was on and left it
for about 11 min. Nothing changed: no boost off, no USB event, CNTL 0x07,
STATUS 21/00/00/20, MUIC INT 00/00/00, FUNC_STATE only the chip's usual
0 ↔ 14 cycle, the 32-byte status block at 0x0030 all zero, the gauge
Discharging throughout. The gauge current averaged about −0.70 A plugged in
and about −0.55 A after the unplug at about 2000 s (screen state not
controlled), so the charger did not feed the tablet; no fault was latched.
That does not prove electrical isolation inside the Kishi.

Inference, not proven: the Kishi never changed its CC presentation and never
sent PD (no PD contract forms: the chip sits in state 14,
`State_PE_SRC_Wait_New_Capabilities`), so neither our polling nor stock's IRQ
handler has anything to react to. Stock reads the same FUNC_STATE on its IRQ.

Consequence measured the same day: the undetected charger does not stop the
battery-inactivity power-off (`services/korrid/src/idle_power.rs`, 15 min
without input on battery). Boot -4 on 2026-11-27 (394qyl2m) ended at 16:38
with `poweroff requested from client … ('korrid')` at 1044.6 s monotonic,
while the charger sat in the Kishi: the gauge read Discharging, so korrid
treated the tablet as on battery. Leaving the tablet charging through the
Kishi this way powers it off after 15 min idle instead of charging it.

**Tested order: charger first.** Charging with the Kishi works when the
charger is already in the Kishi pass-through before the Kishi is attached (or
at boot): the chip attaches as a sink (state 21) and sink host qualifies. To
charge during play: take the Kishi off, plug the charger into it, put it back.
Unplugging the charger during play works: measured unplug at 1264.26 s, boost
on (verified) at 1264.71 s, Kishi enumerated at 1265.08 s, D-pad working
afterwards.

**Read-error revokes in normal battery play (measured, ix1pnpcl boots,
2026-11-27).** Five revokes in four boots, all `boost off: read error`, each
dropping the Kishi for about 3 s (re-enable after the fixed 3 s retry). Four
of the five failed on the dummy read of 0x0008 (`-6` twice, `-5` twice), one
on FUNC_STATE (`-5`). Stock issues the same dummy read before its status reads
and does not check its result (`process_cc_water`). Since 2026-11-27 (parent
decision, replacing the 2026-11-25 rule) a failed dummy read is logged
(`dummy read failed, ignored as stock: s2mm005 0x0008: <errno>`) and the
sample continues; every real status read (FUNC_STATE, MUIC, charger, gauge)
still revokes at once on failure. The policy test replays both.

The driver now logs FUNC_STATE bytes 1–2 on change (`cc: plug … next …`):
CC1/CC2 plug state and the PD next state, already read every poll and not a
policy input. They are a candidate signal for the next attempt.

**Checking removal on battery after the fact.** The Kishi coming off while we
boost must turn boost off within 2.5 s (state-0 episode limit). The persistent
journal shows it; pick the boot with `journalctl --list-boots`:

```sh
journalctl -k -b 0 -o short-monotonic --no-pager \
  | grep -E 's2mm005.*boost (on|off)|usb 1-1: USB disconnect' \
  | awk -F'[][]' '{now = $2 + 0}
      /boost on/ {t = 0}
      /USB disconnect/ {t = now}
      /boost off: / && !/mode 5|NOT verified/ {
        why = $0; sub(/.*boost off: /, "", why)
        printf "%.3f boost off: %s; ", now, why
        if (t) printf "USB disconnect %.3f s before\n", now - t; else print "no disconnect before"
        t = 0 }'
```

A removal prints `boost off: detach` with the USB disconnect at most about
2.5 s before it. A longer gap is a failure; so is a disconnect with no later
`boost off` while boost was on. A `read error` revoke happens before its
disconnect (the revoke powers the Kishi down), so it prints "no disconnect
before".

## User-space remainder

`tabs6-typec-auto` is removed. No USB role service polls chip registers or writes an
automatic role. The old observer is not started; its diagnostic CLI is usable
only while the controller is unbound. `tabs6-typec-role-watch` reads **only**
`/sys/class/usb_role/*/role` every 50 ms. On device-to-host it stops the
gadget; on host-to-device it restarts it (`tabs6-usb-role gadget-start`). It
never writes a role or any chip register, and it does not restart input
providers.

The provider restart (InputPlumber and inputd on every kernel detach) was
removed twice on 2026-11-25. The second removal is an owner decision after it
caused a worse failure. At 226.90 s a single poll read returned -EIO, the
driver revoked boost and the restart began. The kernel re-enabled boost at
230.10 s and the Kishi re-enumerated at 230.47 s, while InputPlumber was still
stopping. The new InputPlumber started at 230.92 s, and its startup scan cached
event6 before udevd had written its database (written at 232.82 s): 5
properties, no `ID_INPUT_JOYSTICK` or `ID_PATH`. The Kishi config never
matched, so there was no composite and no input until the next replug.
`inputplumber-startup-udev-init.patch` makes startup discovery wait for udev
(VM check `inputplumber-udev-startup-vm`).

Case B (#629) is not reproduced since the shell fix, and its cause is not
proven. The dead-input failure found on 2026-11-27 was in the shell: Sway maps
every client unfocused (`no_focus [all]`) and korrid then focuses the shell,
so the window reports focus lost then regained. The router dropped every
gamepad on the loss, and the native-input thread read both changes at once and
never reconnected, so korrid's frames reached a mapper that knew no device
(fixed in `clients/shell/src/input/native.rs`, closed-loop verified). A USB
re-add alone does not cause a shell focus change, so case B (re-adds on
2026-11-25, input dead after some, providers' restart recovered it) may be a
different failure; that boot had no shell trace. On 394qyl2m a re-add on
battery (charger unplug, 1264–1265 s) kept input working. A replug that does
not restore shell input is evidence of case B; capture `KORRI_INPUT_TRACE`
(korrid and shell) before restarting anything.

The driver logs each change of the qualifying facts (`facts: state … flags …
vbus … cntl … (qualified n/3)`, rate limited) and `qualified 3/3`, so the time
from physical replug to SNK_Ready and to host is visible in the journal.

The former manual `tabs6-usb-role host`/`device` commands (unbind the driver,
then boost from user space) were removed on 2026-11-25: the kernel now powers
the Kishi on battery itself, and one boost owner is simpler and safer than two.
`tabs6-usb-role` keeps `status`, the gadget actions, `boot-role` and
`boot-vbus`; it has no command that enables boost.

Kernel host qualification can finish before systemd starts the gadget.
The role watcher starts without waiting for the gadget. On an initial or new
host role it stops the gadget (with a three-second command deadline), which
cancels its late-UDC wait and avoids delaying network startup for 60 seconds.
It clears the intentional stop's failed state only after the stop job completes;
a pending oneshot can otherwise retain `Result=signal`. On return to
device it restarts the gadget. This keeps the shared product gadget unchanged;
only the Tab S6 edge knows about its role switch. No role or chip write is part
of either gadget lifecycle action.

`tabs6-usb-role-boot` (sysinit, as on main) still writes `device` before the
gadget binds. It runs about 10 s before the module loads and refuses once the
driver is bound, so it never competes with the kernel owner. Removing it was
the only early-boot userspace difference in the builds whose counted trials
fell back (see "Open risk"); it was restored for that reason.

The existing boot `boot-vbus` service still requests verified boost-off because
SM5705 registers can survive warm reboot. The boot-role writer runs on both systems. Nothing here changes charge-current maintenance.

## Replug latency (measured 2026-11-25)

Replug test on `1gabybg1` (boot 74593cd9 series, monotonic seconds, driver
`facts:` lines and InputPlumber at debug level):

| Event | Time | Owner |
| --- | --- | --- |
| Charger unplugged: VBUS 00, device | 188.83, 188.84 | chip, driver (one poll) |
| Unplugged, the chip re-runs detection every ~15.8 s (state 0 → 14) | 204.2, 219.9, 235.7 … 377.8, 393.9 | s2mm005 firmware |
| Charger replugged: MUIC VBUS 01 | 380.08 | SM5705 |
| Chip still in state 14 until its next cycle | 380.08 → 393.86 (13.8 s) | s2mm005 firmware |
| 17 (wait caps) → VBUS dip (pass-through) → 20 → 21 SNK_Ready, DFP=0 | 395.05 → 396.80 | chip, charger, Kishi |
| DFP=1 | 398.72 (1.9 s after SNK_Ready) | chip/PD |
| `qualified 3/3`, host | 398.94 | driver (0.22 s) |
| USB enumerates; InputPlumber re-adds evdev://event9 into the surviving CompositeDevice0, no EBUSY, no restart | 399.31; 400.25–400.63 | kernel, InputPlumber |

The driver's part is about 0.2 s and the input stack about 1.7 s. Almost all of
the ~20 s Simon saw is the s2mm005 firmware's ~15.8 s detection cycle: VBUS can
arrive anywhere in it, so replug latency varies between about 3 s and about
18 s by phase (inferred from one replug; the earlier ~9 s with the old daemon
fits a luckier phase). Shortening the cycle would need chip configuration
writes (low-power or DRP settings), which this driver must never send. Treat
the cycle as a hardware limit unless the owner decides otherwise.

Case B (source re-adds but no shell input) did not reproduce: the highlight
moved, with the InputPlumber teardown fix and no provider restart.

## Late module load (2026-11-24)

The driver is a module (`CONFIG_TYPEC_S2MM005_READ_ONLY=m`). It is
blacklisted from udev alias autoload, and `tabs6-typec-kernel-policy.service`
loads it with `modprobe` after `systemd-journal-flush`,
`tabs6-usb-role-boot-vbus` and the gadget unit. This restores the old daemon's
ordering. dwc3 stays peripheral (DT default) until the load, so host arrives
roughly 6–12 s later at boot. Probe requests device as soon as it has the role
switch, before reading any fact, so every later probe failure leaves device.
Probe start, probe end and the first poll's result are logged, so a hang
inside the driver leaves its last line in the persistent journal.

Manual `host` refuses until that unit is active (otherwise a late probe would
take over the override). `device` rebinds the driver, or loads the module
again if it was removed.

Reason: the built-in driver probed at 4.85–5.47 s. Two counted trials fell
back with no journal at all, so they died before the journal flush
(5.1–5.8 s on good boots), after systemd armed the qcom_wdt watchdog (about
4.9 s, 30 s timeout; with panic timeout 0, only the watchdog can reset a hung
kernel into LAST KNOWN GOOD). The ESP files were byte-identical to the store
and systemd-boot had chosen the trial entry. The driver is a plausible but
unproven cause.

I2C concurrency was checked in code: `tabs6-usb-role boot-vbus` and
`tabs6-charge` use `i2cget`/`i2cset` without `-f` (`I2C_SLAVE`). No Linux
driver claims 0x49 or 0x25, and their SMBus calls take the adapter lock in
`i2c_smbus_xfer`. The driver's `i2c_transfer` takes the same lock. GENI GPI DMA
and FIFO transfers both run inside the locked `master_xfer`. Nothing bypasses
the lock in Linux. Firmware in another execution environment using SE4 would
be outside Linux locking; this was not checked.

## Open risk: unexplained trial fallbacks (2026-11-24/25)

Bisect, all counted trials with deploy-over-ssh (ESP write, then reboot):

| Kernel / DT | Userspace | Result |
| --- | --- | --- |
| mine (graph or phandle link) | mine (some with diagnostics) | 7 of 14 fell back |
| LKG kernel and DTB, byte-identical | mine | 1 of 2 fell back |
| LKG kernel and DTB | main `6911f5bac` | 2 of 14 fell back |

Main fails too, so the reset exists without this work. Mine against main is
8/16 against 2/14 (one-sided Fisher p ≈ 0.045): suggestive, not proven. Open
question: does the shipped closure fall back more often than main? Measure it
with about 10 counted trials of the landed main. Restoring the sysinit
`tabs6-usb-role-boot` (1 fallback in 5) and the late module load did not
remove it.

LKG itself also failed one boot (boot `daa4c84c`, 2026-11-25 02:54, journal
ends at 7.95 s during zram setup, no clean shutdown), directly after a failed
trial's watchdog reset. So a background early reset exists on this tablet.
Failed boots leave no journal (or one cut off within seconds) and no pstore
record; the qcom_wdt watchdog (armed at about 4.9 s, 30 s) resets them into
LAST KNOWN GOOD.

Earlier notes:

Two counted trials of the built-in driver fell back to LAST KNOWN GOOD (first
mzjc, then s19cnbas). Neither left journal entries or a pstore record (only
EFI pstore is registered; no ramoops region is configured). Two other trials
(mzjc again, lagbqp) booted and were blessed. The cause is unknown; see "Late
module load" for the evidence and the mitigation. Do not treat it as explained. If it recurs, capture
the screen and consider a ramoops region or a late-bound module for diagnosis.

## Verification

`tabs6-typec-policy` compiles the exact header included by the kernel driver.
Sink host: it replays Phase A, charging, the old sourcing traces and both 100 %
gauge traces (host must hold, boost never requested) and exhausts PD states,
flag bytes and MUIC/CNTL bytes. Battery source: it replays the measured
unpowered 0 ↔ 14 cycle with a charger replug (first enable on a 14 seen twice,
never with VBUS 01) and the measured boost-on (state 0 for ~0.7 s, then 14 with
attached 0; boost must hold through and after the window). It exhausts every
enable interlock and covers every revoke path, in and after the window:
charger insertion as 17/20/21/18/29/52 with sink host requalifying, detach,
read error, CNTL leaving mode 7, OTGFAIL, VBUSOVP, VBUSLIMIT, wireless input,
MUIC charger/PC signatures, unknown states, failed starts with 3 → 60 s
backoff and its reset, and a failed boost-off that stops the policy.
`tabs6-usb-role` covers the gadget actions, boot-role, boot-vbus and that
user space has no boost-enabling command.

Build and deploy from the workstation, never the tablet:

```sh
nix build .#checks.x86_64-linux.tabs6 .#checks.x86_64-linux.tabs6-usb-role \
  .#checks.x86_64-linux.tabs6-typec-policy
nix build .#tabs6-deploy-over-ssh
$(nix build --no-link --print-out-paths .#tabs6-deploy-over-ssh)/bin/deploy-over-ssh \
  --port 2222 --reboot root@192.168.1.119
```

Use the counted boot and LAST KNOWN GOOD path only. Inspect the driver, Type-C
port, CNTL and the driver `facts:` lines with `journalctl -o short-monotonic`.
Do not claim shell-input acceptance from a USB enumeration or a Ready log alone.
