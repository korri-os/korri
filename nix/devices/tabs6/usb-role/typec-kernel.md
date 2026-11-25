# Kernel-owned Tab S6 USB-C data role

The Korri product uses `s2mm005-read-only` (Linux 7.3.4). The console kernel
stays unchanged and has only the existing manual role helper. There is no
user-space automatic role policy or fallback.

## Ownership and safety

The driver selects dwc3 roles through `usb_role_switch_set_role`. It registers
a standard Type-C port and an attached partner. The Type-C data role reports
the selected Linux data role. The power role reports the observed s2mm005
source flag; it does not command power. No Type-C swap operations are offered.
The driver does not infer cable orientation, a PD contract or a specification
revision from the observed fields.

A delayed work item reads every 100 ms after the preceding sample completes.
Three consecutive fresh samples permit host only when **all** of these hold:

| Interlock | Shipped implementation |
| --- | --- |
| PD state 21 | `sample.state == 21` |
| Attached, source=0, DFP=1 | `(sample.flags & 7) == 5` |
| MUIC VBUS exactly 01 | `sample.muic_vbus == 1`, not just bit0 |
| Charger CNTL mode 5 | `(sample.cntl & 7) == 5` |
| Three fresh successful reads | saturating counter in `s2mm005-policy.h` |

Any nonqualifying sample or read error selects device and resets the counter
on the same work invocation. Only enables wait. A normal PC/UFP never qualifies.
Role-set errors reset qualification and force a device request. Each poll also compares the decision with the switch's actual role. The
role file stays writable (dwc3 allows user-space control), so an outside write
is logged (`role changed outside policy`) and undone on the next poll. This
was added after another session's old daemon wrote the role 293 times during
an overlapping deploy on 2026-11-24. Probe, remove,
shutdown and suspend request device. Resume starts qualification from zero.
The API schedules dwc3's role work; a successful request is not proof that
controller teardown has completed.

The driver performs **only address-plus-repeated-START reads**. It never writes
register data to any chip, sources VBUS, touches boost, requests PD or power
contracts, swaps roles on the controller, accesses firmware/flash/OTP, masks
interrupts, acknowledges interrupts or clears them. It does not even read
interrupt status 0x0030 or the MUIC read-to-clear latches. IRQ123 is not requested:
it can remain asserted without acknowledgement, so a polling safety path is
required anyway. Polling costs periodic wakeups; 100 ms is a target interval,
not a hard real-time deadline. A slow/failing I2C controller can delay a sample.

### Why the fuel gauge is not an interlock (owner decision, 2026-11-24)

The previous user-space policy and the first kernel builds also required gauge
Charging. At 100 % battery with the charger in, that check revoked host by
itself while the chip state held still:

| Trace | Gauge | MUIC VBUS / CNTL / PD | Effect of a gauge interlock |
| --- | --- | --- | --- |
| `sink-host-full-flap.txt` | Charging → Full for ~0.6 s, every ~30 s (+69 mA) | 01 / 0x05 / unchanged | host → device → host, ~6 s input loss per provider restart |
| `sink-host-full-discharging-dips.txt` | Full → Discharging for 0.2–0.4 s (current around the 50 mA deadband) | 01 / 0x05 / unchanged | five device dips in about 4 minutes |

`sm5705_fg` derives status from current alone: Charging above +50 mA,
Discharging below −50 mA, otherwise Full (SOC ≥ 99) or Not charging. With a
full battery the charger tapers, so the current crosses the deadband and the
gauge cannot tell "charger in, battery full" from "on battery".

The gauge was a redundant power check. `typec-phase-b.md` records that this
board sources VBUS only through SM5705 mode 7, never through the dwc3 role
switch. While CNTL reads mode 5, MUIC VBUS=01 together with SNK_Ready,
attached and source=0 therefore means the charger supplies the port, and the
data role cannot source it. The policy keeps those six checks, three fresh
samples and immediate revocation on any of them or on a read error. Manual
host and the VBUS sourcing rules are unchanged; manual host still requires a
Discharging gauge. The driver no longer depends on the gauge driver at all.

## Register and resource grounding

The vendor source and stock DT evidence are listed in `typec.md`, at Samsung
commit `37cf30da0578a6378a3e3ef5e17aede89233defe`.

| Read/resource | Grounding and choice |
| --- | --- |
| s2mm005 0x33 on `&i2c15`, 0xc94000 | Stock DTBO fragment 130 and boot symbols. A product-only client node uses `samsung,s2mm005-read-only`. No vendor probe/initialization code is ported. |
| 0x0008, one byte | `s2mm005.h` SW_VERSION_1, ordinary dummy/status read retained from the observer. No firmware access command. |
| 0x0020, four bytes | `s2mm005.h` FUNC_STATE_Type and `s2mm005_cc.c` process_cc_attach. Byte0 is state; byte3 bits0/1/2 are attached/source/DFP. |
| Two-byte address, MSB first | `s2mm005.c` s2mm005_read_byte. Exactly two completed I2C messages are required; short transfers are errors. |
| SM5705 adapter `&i2c4`, 0x890000 | Stock DTBO fragment 133. `samsung,sm5705-i2c` phandle resolves the adapter, never a Linux adapter number. |
| MUIC 0x25, 0x15, one byte | `muic_regmap_sm5705.c`, REG_RSVDID1 / RSVDID1_VBUS_VALID. The existing policy requires the whole byte to equal 01. |
| Charger 0x49, CNTL 0x0c, one byte | `include/linux/mfd/sm5705/sm5705.h`; bits2:0 mode 5 = CHG_ON. |
| dwc3 | Standard reciprocal OF graph endpoints link the controller to `&usb_1_dwc3`; `usb_role_switch_get` defers until the switch exists. Its default is peripheral. |

The charger and MUIC addresses are deliberately **not** reserved with dummy
clients. They have no kernel drivers. Adapter-locked combined reads coexist
with the unchanged charge-maintenance and boot boost-off i2c-dev services,
which would otherwise fail with EBUSY. No forced I2C access is used. This board
specific driver must be revisited if either address gains a kernel owner.
Cross-chip reads are sequential, as before; they are not a hardware-atomic
snapshot. Every poll gets new facts, and no observer log grants permission.

## User-space remainder and manual override

`tabs6-typec-auto` is removed. No USB role service polls chip registers or writes an
automatic role. The old observer is not started; its diagnostic CLI is usable
only while the controller is unbound. `tabs6-typec-role-watch` reads **only**
`/sys/class/usb_role/*/role` every 50 ms. On host-to-device it invokes
`tabs6-usb-role detached`. That command waits for device role and zero host USB
devices, restarts the gadget, then restarts InputPlumber and inputd. It never
writes a role or any chip register. This is the temporary InputPlumber
workaround, not role policy. Kernel qualification does not wait for user-space
provider cleanup. A rapid replug can therefore race cleanup; the physical
acceptance sequence must check shell input, not enumeration alone.

The manual battery-powered Kishi path remains available:

```sh
tabs6-usb-role host    # stop role watcher, unbind kernel policy, existing checks/VBUS sequence
tabs6-usb-role device  # verify boost off, device/gadget, rebind kernel policy and restart watcher
```

The existing OF compatible identifies the one client even after unbind. No
persisted override flag is introduced. Unbind synchronously cancels work and
requests device before releasing the port. Only then can the existing manual
helper read s2mm005 and perform its unchanged Discharging/source qualification
and VBUS-first sequence. A failed manual host attempt stays unbound; run
`device` to verify boost-off and resume policy. Reboot also restores the driver.
Do not change cables in manual powered-host mode: it remains an unmonitored
operator override, not proof of safe pass-through sourcing.

Kernel host qualification can finish before systemd starts the gadget.
The role watcher starts without waiting for the gadget. On an initial or new
host role it stops the gadget (with a three-second command deadline), which
cancels its late-UDC wait and avoids delaying network startup for 60 seconds.
It clears the intentional stop's failed state only after the stop job completes;
a pending oneshot can otherwise retain `Result=signal`. On return to
device it restarts the gadget. This keeps the shared product gadget unchanged;
only the Tab S6 edge knows about its role switch. No role or chip write is part
of either gadget lifecycle action.

The existing boot `boot-vbus` service still requests verified boost-off because
SM5705 registers can survive warm reboot. The product has no user-space
`boot-role` writer; the driver owns boot data role. The console retains its
old manual boot-role service. Nothing here changes charge-current maintenance.

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

## Open risk: unexplained trial fallbacks (2026-11-24)

Two counted trials of the built-in driver fell back to LAST KNOWN GOOD (first
mzjc, then s19cnbas). Neither left journal entries or a pstore record (only
EFI pstore is registered; no ramoops region is configured). Two other trials
(mzjc again, lagbqp) booted and were blessed. The cause is unknown; see "Late
module load" for the evidence and the mitigation. Do not treat it as explained. If it recurs, capture
the screen and consider a ramoops region or a late-bound module for diagnosis.

## Verification

`tabs6-typec-policy` compiles the exact header included by the kernel driver.
It replays all six real traces (Phase A, charging, sourcing flap, bounded
shutdown, and both 100 % gauge traces, which must hold host with no role
change), exhausts all PD states/flag
bytes and MUIC/CNTL bytes, checks each revocation and error/reset, and keeps a
detached Kishi in device for 600 samples (60 s at the target period).
`tabs6-usb-role` retains the manual interlock/write-order/failure tests and adds
unbind/rebind and real read-only watcher/detach tests. Removed tests exercised
the deleted automatic sourcing candidate, not permission in the shipped kernel.

Build and deploy from the workstation, never the tablet:

```sh
nix build .#checks.x86_64-linux.tabs6 .#checks.x86_64-linux.tabs6-usb-role \
  .#checks.x86_64-linux.tabs6-typec-policy
nix build .#tabs6-deploy-over-ssh
$(nix build --no-link --print-out-paths .#tabs6-deploy-over-ssh)/bin/deploy-over-ssh \
  --port 2222 --reboot root@192.168.1.119
```

Use the counted boot and LAST KNOWN GOOD path only. Inspect the driver, Type-C
port, CNTL and input-provider journals with `journalctl -o short-monotonic`.
Do not claim shell-input acceptance from a USB enumeration or a Ready log alone.
