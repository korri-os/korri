# Phase B: permanent Kishi V2 with optional pass-through charger

## Current owner: read-only kernel policy

The Korri product now uses `s2mm005-read-only`, not `tabs6-typec-auto`.
`typec-kernel.md` describes the exact unchanged sink-host interlocks, 100 ms
sampling, standard Type-C port, read-only resource access, user-space detach
workaround and manual override through driver unbind/rebind. There is no
user-space automatic policy or fallback. The kernel policy no longer uses the fuel
gauge: the gauge interlock below (Charging) flapped at 100 % battery with the
charger in, and the chip checks already prove incoming power (owner decision
2026-11-24; reason and measurements in `typec-kernel.md`). The policy/history below records
prior user-space implementation and measurements, not current instructions.

## Previous user-space policy: charging host data role only (2026-11-24)

`tabs6-typec-auto` may select the **host data role with boost off**. Automatic
VBUS sourcing remains blocked by the unchanged runtime interlock below.
Manual `tabs6-usb-role host` retains its existing behavior.

Grounding: the live Kishi pass-through trial measured PD_State 21
(`SNK_Ready`), ATTACH_DONE=1, IS_SOURCE=0 and IS_DFP=1 in 13,213 of 13,233
samples. Selecting only dwc3 `host`, without touching SM5705 boost, enumerated
1532:0712 and carried shell input while the gauge read Charging at +0.8–1.1 A.
CNTL stayed 0x05. This board sources VBUS only through SM5705 mode 7, never
through the dwc3 role switch. `sink-host-charging.txt` preserves eight further
read-only observer samples taken before implementation, including STATUS1=29.
That STATUS1 value still denies **sourcing**; it is not a data-role interlock.

The separate sink-host policy needs three consecutive fresh reads with **all**
of these values: state 21, attached, source=0, DFP=1, MUIC VBUS=01, gauge
Charging, CNTL mode 5. It selects device on every other sample or read error.
The serialized automatic-only `sink-host` helper checks fresh status before
and after the role write. It never writes charger registers, including on a
failed post-switch interlock. A failed check logs the failure, restores the
device data role, and rebinds the gadget. The daemon also requests its existing
off/device cleanup on failure. Unexpected boost, service stop and restart keep
the existing off/device recovery.

Charger unplug must remove the host controller by returning to device role.
Replug must requalify from zero. This full data-role cycle matters: the live
manual-host experiment left the host controller up across charger unplug and
InputPlumber retained a dead source, then failed the evdev re-add with EBUSY.
Role cycling alone did not recover input in the live replug test. The provider
kept the old composite and dead source even after USB re-enumerated. Restarting
InputPlumber and inputd while the host controller was absent recovered shell
navigation without reboot (same boot ID). The automatic `sink-device` path
therefore requests off/device, waits up to 3 seconds for device role and zero
host USB devices, then restarts both providers with a 10-second deadline.
Failure stops policy in device role; host qualification cannot run during
teardown. Manual role commands do not restart input providers. Live acceptance
must prove that controls return without a reboot. A normal PC
reports UFP rather than DFP and keeps the gadget lifeline in device role.

Tests replay Phase A's charger transitions (which do **not** yet carry the
complete qualifying signature), the new charging trace, charger unplug,
60 seconds of unplugged samples, read failures and the PC/UFP path. The real
daemon/helper tests prove host/device/host cycling, no sourcing writes, gadget
rebind, and post-switch mode-change/read-error rollback without register writes.

## Previous policy: observe and stay DEVICE (2026-11-22 live safety block)


Automatic HOST is **blocked**. `tabs6-typec-auto` runs at boot and remains
running, but never invokes the helper's `host` command. It starts off/device,
logs fresh status every 300 ms, and requests off/device on read failure,
unexpected boost, or a late dwc3 host-role completion. Its journal says
`mode=observe automatic_host=blocked reason=no_verified_power_direction`.
There is no runtime opt-in to automatic HOST.

This is the parent-approved safe outcome of the flapping investigation. The
required recovery to state 14 / attached 1 / source 1 did **not** occur while
boost was on. USB enumeration does not establish safe power direction. A longer
settle window, accepting source=0 indefinitely, or using successful enumeration
as power permission would violate the owner's negligible-risk constraint.
The charger plug/unplug acceptance test was therefore **not performed**.

Manual `tabs6-usb-role host` remains available with its existing fresh interlock
and effect ordering. It stops the daemon and requires Discharging, three source
checks, no incoming VBUS, and no configured gadget. The new checker also rejects
charger faults, unknown STATUS1 values and MUIC charger/PC device signatures.
This remains an **unmonitored operator override**, not proof of safe pass-through
operation. Select `device` before changing cables, then restart the daemon.

### Measured sequence, charger unplugged, Kishi taped on

`phase-b-host-transition.txt` contains all 51 raw policy samples, including the
three qualification samples and the first 5 s after HOST. It uses the actual
journal producer's format, including elapsed time, dwc3 role, CNTL, boost,
MUIC device types and sample duration. The target period was 100 ms; subprocess
reads and actuation added scheduling jitter. No interrupts or firmware commands
were sent. `phase-b-flap.txt` preserves the original daemon's premature shutdown.

VBUS-first trial, kernel monotonic time 4668.688671 s at verified HOST:

| Time after HOST | Observed event |
| --- | --- |
| 95 ms | state 0 / attached 0 / source 0; CNTL=07, boost=0b, MUIC VBUS=01, STATUS1=21; dwc3 still reports device |
| 174 ms | Same power/CC status; dwc3 reports host |
| 267–585 ms | state 0 / attached 1 / source 0 |
| 379 ms | `usb 1-1: new full-speed USB device number 2 using xhci-hcd` |
| 535 ms | First `Razer Razer Kishi V2` input node appears; HID interfaces follow |
| 686 ms | state 0 / attached 0 / source 0 |
| 794 ms onward | state 14 / attached 0 / source 0; no source=1 recovery |
| 3103 ms | Settle deadline expires; DEVICE requested, retry delayed 3 s |
| 3225 ms | `role device, VBUS off (readback verified)` |

Throughout the powered portion, STATUS1=0x21, MUIC VBUS=01 and device types
were 00/00/00. Gauge remained Discharging. After off, STATUS1 returned to 0x22.
The first safe-mode reboot also exposed MUIC DEVT1=0x08 / DEVT2=00 / DEVT3=00
with state 14/1/1, MUIC VBUS=00, STATUS1=0x22, CNTL=05, boost=05 and
Discharging. Samsung names DEVT1 bit3 `UART`; it is not a charger/PC power
signature. The checker accepts this measured passive classification so the
existing manual interlock remains usable after reboot. This does not unblock
automatic HOST. A regression test runs the actual manual helper with these
readings, and observe-mode tests prove this classification still cannot enable
HOST automatically.

The Kishi disconnected when boost went off. Sustained HOST and inputd Ready
were **not accepted**; transient input nodes alone do not satisfy acceptance.

A second experiment requested dwc3 host first and waited for host readback
before enabling boost. Host readback was at 4760.614866 s; verified boost-on
was at 4760.920779 s. It produced the **same** 0/0/0 → 0/1/0 → 14/0/0 sequence,
with 14/0/0 first sampled at 787 ms. It timed out at 3104 ms and verified off
at 3239 ms. Changing order did not restore the source flag. The experimental
helper change was removed; the shipped manual helper still powers VBUS first.

### STATUS1/MUIC do not prove direction

Grounding: Samsung commit `37cf30da0578a6378a3e3ef5e17aede89233defe`,
`include/linux/mfd/sm5705/sm5705.h` (`SM5705_INT_STATUS1_*`) and
`drivers/muic/universal/muic_regmap_sm5705.c` (`DEVT1_*`, `RSVDID1_VBUS_VALID`).

| Register value | Meaning, not a power-direction claim |
| --- | --- |
| STATUS1 0x22 (decimal 34) | VBUSUVLO + WPCINUVLO; measured boost-off baseline |
| STATUS1 0x21 (decimal 33) | VBUSPOK + WPCINUVLO; measured **both** for our OTG output and charger input |
| STATUS1 0x29 (decimal 41) | 0x21 plus VBUSLIMIT; revoke as a fault, not as an OTG/input discriminator |
| STATUS1 0x15 (decimal 21), if reported | VBUSPOK + VBUSOVP + WPCINPOK; reject; not an OTG direction flag |
| MUIC VBUS_VALID bit0 | Voltage present on the shared port; rises for our output too |
| MUIC DEVT1 bit7 (USB_OTG) | Device classification, not proven independent direction; absent in these trials |
| MUIC DEVT1 bit3 (UART) | Passive classification observed after the safe-mode reboot, with no VBUS; not power direction |

There is no dedicated OTG-versus-external-input direction bit in the inspected
STATUS1/MUIC definitions. VBUSOVP/LIMIT, wireless input and MUIC charger/PC
signatures can revoke permission, but their absence cannot prove power direction.

### Bounded candidate policy retained for regression tests

The pure policy implements a fixed 3 s settle window after verified actuation.
Only measured redetection states (0 with source=0, then 14 with both flags
clear) may wait while our boost is verified on. Read failures, Charging,
sink attachment (including 17/20/21), charger faults, MUIC charger signatures,
unknown states or lost boost revoke immediately. The window never extends on
another sample. A start that does not recover state 14 / attached 1 / source 1
requests DEVICE and waits 3, 6, 12, 24, 48, then at most 60 s before another
qualification. Read errors cannot erase backoff. Only 60 s of confirmed HOST
resets backoff. This candidate is **not permission to actuate**: the daemon
blocks every candidate HOST action and stays DEVICE.

Tests replay both live fixtures and Phase A, cover the exact deadline and
backoff cap, and exercise the real daemon with file-backed I2C commands. They
prove that the shipped daemon never enables HOST even with permanent source
qualification, but removes unexpected boost or host role. All manual helper
interlock, failure-cleanup and write-order tests remain.

### What would unblock automatic HOST

Either of these needs evidence before automatic HOST can return:

- A validated, independent s2mm005 power-direction signal that remains usable
  with our boost on and changes promptly on pass-through charger insertion.
  Inspect Samsung's power-role/PD-contract handling, source/VBUS status and
  PD/DFP sourcing indications. The source contains `Port_Power_Role`,
  `FUNC_STATE.IS_SOURCE`, `FUNC_STATE.IS_DFP` and source-status handling;
  these are investigation candidates, **not** permission. DFP alone is a data
  role and must never substitute for power direction. Do not send PD requests,
  swaps, firmware commands or new configuration writes to obtain evidence.
- Evidence for how the Kishi V2 pass-through electrically isolates an attached
  charger from the phone port while the phone sources VBUS. Enumeration and
  a Discharging gauge do not prove this isolation.

Keep the daemon running in observe mode until that evidence exists. Do not run
the charger acceptance test against the unproven powered-host signature.

## Earlier implemented policy (superseded by the live safety block above, 2026-11-22)

This measured policy supersedes the broader proposed policy below. Phase A's
journal was collapsed on the actual tablet, not inferred from Samsung enums:

| Signal | Kishi, no charger (61,666 samples) | Pass-through charger |
| --- | --- | --- |
| s2mm005 state / attached / source | 14 / 1 / 1 | 21 / 1 / 0 (transients 0, 20, 42) |
| MUIC VBUS | 00 | 01 |
| STATUS1 | 22 | 21 or 29 |
| Gauge | Discharging | Charging (may lag the role transition) |

`tabs6-typec-auto` samples fresh status every 300 ms. It enables only after
three consecutive state=14, attached=1, source=1, MUIC VBUS=00, VBUSPOK=0,
gauge!=Charging samples with every read successful. The helper takes two more
fresh checks around its configuration writes. Read errors, state!=14,
source=0, attached=0 or Charging revoke immediately. Incoming VBUS revokes
while boost is off. While our boost is on, its own VBUS indicators are not
incoming-power evidence: the MCU source role is authoritative. Boot/restart
cannot inherit sourcing permission. No interrupts are acknowledged.

The existing helper owns all writes, serialized with flock. It resolves the
SM5705 adapter by OF name, preserves the existing boost/current settings,
checks all I2C exits, verifies CNTL readback and retries disable three times.
It requests boost-off before device, even if disable cannot be verified.
Failed actuation stops automatic policy; systemd ExecStopPost also requests
off/device on normal stop, failure or process death. A manual host/device
command stops the daemon before taking its lock; `systemctl start
tabs6-typec-auto` resumes it. Manual host retains the stricter Discharging
check and now also needs three measured source samples. Manual host is an
unmonitored operator override, not a second automatic policy.

The daemon blocks logind suspend while running. Polling is the safety path;
GPIO status remains in the separate observer. Each I2C subprocess has a 200 ms
timeout. Total I2C loss can still prevent physical boost-off: a failure is
reported, never represented as verified VBUS-off.

## Earlier design and acceptance inventory (reference, not the active policy)

## What the source establishes

Samsung source commit and local stock DT evidence are recorded in `typec.md`.
These values come from `function_status_t` in `include/linux/ccic/s2mm005.h`:

| PD state | Samsung name | Stage 1 treatment |
| --- | --- | --- |
| 0 | State_PE_Initial_detach | VBUS off, device |
| 1 | State_PE_SRC_Startup | Source candidate only; never sufficient alone |
| 2 | State_PE_SRC_Discovery | Not in Samsung's VBUS-on switch cases; deny |
| 3 | State_PE_SRC_Send_Capabilities | Source candidate only |
| 4 | State_PE_SRC_Negotiate_Capability | No PD handling in stage 1; deny |
| 5 | State_PE_SRC_Transition_Supply | No supply transition in stage 1; deny |
| 6 | State_PE_SRC_Ready | Do not infer that a negotiated contract is safe; deny until measured |
| 7 | State_PE_SRC_Disabled | Source candidate only; non-PD sink may settle here |
| 8..14 | Other source policy-engine states, including resets | Ambiguous for this slice; deny |
| 15..25 | Sink policy-engine states | VBUS off, device |
| 17, 18, 21 | SNK_Wait_for_Capabilities, SNK_Evaluate_Capability, SNK_Ready | Samsung explicitly selects the sink/device path |
| 26..28 | Cable VDM identity states | No alternate modes or VDM handling; deny |
| 29 | State_ErrorRecovery | VBUS off, device |
| 52, 64 | Power-role swap transition states | Never handle or initiate swaps; deny |
| Other | Unknown | VBUS off, device |

Samsung's `process_cc_attach` uses source switch cases 1, 3, 4, 5, 6, 7 and
calls `vbus_turn_on_ctrl(1)`. This is reference evidence, **not** permission to
copy that policy: it also participates in PD, swaps and alternate modes,
which this slice must not port.

`ATTACH_DONE` (bit24) and `IS_SOURCE` (bit25) are the relevant independent
flags. `IS_DFP` (bit26) is a data-role flag, not a power permission. The real
charger baseline is `15 21 ff 9d`: source=0 **but DFP=1**. An implementation
that treats DFP as permission would be unsafe. `RESET` is set in that baseline;
it cannot be assumed to be an instantaneous reset pulse. The next-state byte
is 0xff there. Neither field is permission to source power.

The inspected Samsung header names `CC1_PLUG_STATE` and `CC2_PLUG_STATE` but
does not define their numeric CC termination encodings. The baseline charger
reports CC1=1, CC2=2. Do not assign these numbers invented Rp/Rd meanings.
Only accept a source pattern after the Kishi's unpowered attach and subsequent
detach demonstrate it. If no attached-sink state is observable read-only,
Phase B stays blocked. Do not enable DRP or acknowledge interrupts to make
Phase A pass.

## Registers and interrupts

Retain the fixed status-read interface, with adapters resolved by OF node:

- s2mm005, 0xc94000, 0x33: ordinary dummy/status read 0x0008; FUNC_STATE
  0x0020 (4 bytes); IRQ status 0x0030 (32 bytes); low-power/water state
  0x0060 (4 bytes).
- SM5705, 0x890000: charger 0x49 STATUS1 0x08; MUIC 0x25 VBUS_VALID
  0x15 bit0, charger type 0x17, device types 0x0a..0x0c. Gauge status
  comes from the existing sm5705-fuelgauge power_supply driver.
- GPIO-cdev inputs: TLMM GPIO123 (s2mm005), GPIO122 (MUIC), GPIO41 (charger).
  Resolve the chip by its OF node, not its enumeration number.
- MUIC IRQ latches 0x03..0x05 clear on read. Do not configure masks or AFC.
- Samsung's `s2mm005_int_clear` sends exactly `00 10 01` to address 0x33.
  This is an ordinary interrupt acknowledgement, not a firmware operation.
  It is a **Phase B-only candidate**, not yet used or proven necessary. Do
  not add a generic write function or CLI. Do not copy reset, initialization,
  LPM, boot-mode, firmware, flash, OTP, alternate-mode or swap paths.

GPIO123 was input with a pull-down before deployment; GPIO41 and GPIO122 were
inputs without pulls. The Phase A observer keeps those biases unchanged.
Capture will determine whether IRQ123 remains asserted without acknowledgement
and whether status still updates. No IRQ edge is not evidence of no detach.

## Proposed effect boundary

Use one owner for automatic role and boost control. Keep the existing SM5705
VBUS sequence and current limit, with its direct gauge and configured-UDC
checks. Do not consume the observer's cached logs or add a flag file granting
permission to source power. A manual `host` call must pass the same fresh
checks as an automatic call; it must not bypass policy.

Before **every** enable, all of these must hold simultaneously:

1. A fresh, measured s2mm005 attached-sink pattern, source bit set, a known
   non-PD source state, no short or water indication, and the dry flags valid.
2. Charger input status unambiguously absent: VBUSUVLO set, VBUSPOK/OVP and
   input-limit fault bits clear. Also no wireless input indication.
3. MUIC VBUS_VALID clear and no charger/PC indication. Unknown or conflicting
   MUIC values deny permission; their actual idle values still need capture.
4. Gauge status exactly Discharging. Missing, unreadable, Charging, Full,
   Not charging or unknown all deny permission. Keep the existing stricter
   check, not only a comparison with Charging.
5. No configured gadget/host on the UDC.
6. A consistent second s2mm005/power snapshot immediately before the CNTL
   write that enables boost. Configuration writes preceding that write must
   not substitute for this re-check. Read errors or changes revoke permission.

On detach, incoming power, gauge change, short/water, inconsistent reads,
monitor failure, service stop or process failure: request VBUS off first,
then device role, then gadget rebinding. Serialize manual and automatic calls.
Check every I2C exit code explicitly: shell `set -e` is not reliable inside
functions used as conditions. Confirm boost is off by readback, retry failed
disable operations and do not claim success if they fail. Keep device role
even if I2C is unavailable. Never retry an enable after a partial failure.

Use GPIO edges for prompt revocation and a short periodic safety re-check as
backup. An edge alone is never permission to enable. Do not add an arbitrary
attach debounce to shutdown: delay only enables, never revocations. Prevent
suspend while boost is on or arrange an off-before-suspend path, and always
start device/boost-off after a warm reboot.

## Acceptance tests to implement before actuation

| Case | Required externally visible effect |
| --- | --- |
| Real Phase A Kishi attach, both observed orientations | After fresh consistent reads, boost on and host; no PD commands |
| Detach from host | Boost-off write precedes device role and gadget restart |
| Direct charger and PC | Device, no enable writes |
| Charger baseline with source=0, DFP=1 | No enable writes |
| Fake charger: source flags claim attached sink, gauge says Discharging, MUIC VBUS_VALID or charger VBUSPOK says powered | No enable writes; off/device even if previously host |
| Gauge alone says Charging, Full, Not charging, unknown, missing, read error | Off/device |
| Incoming power appears between first check and final enable | No boost-enable write |
| Detach occurs between those checks | No boost-enable write |
| Each status read fails or returns a short/invalid result | Off/device; never use old data |
| Every possible source/attach flag combination and unknown PD state | Only measured attached-sink patterns can enable |
| Water, short, contradictory source and sink facts | Off/device |
| Failure at each SM5705 configuration/write/readback step | No further enable; off/device; explicit failure |
| GPIO monitor exit, daemon termination, crash, warm reboot | Off/device; no inherited permission |
| Adapter renumbering or duplicate/missing OF matches | Resolve uniquely or fail closed; never probe another 0x49 |
| Observe VBUS generated by our own boost | Must distinguish incoming power without weakening checks; if readings cannot do so, stop Phase B |

Use a temporary sysfs tree and file-backed I2C command implementations that
record every operation. Add transitions between calls, not only steady-state
fixtures. Run the actual helper/daemon against those programs. Then exercise
the real hardware under logging: Kishi, detach, charger, PC and the safe
fake-charger refusal path. Never test a source-to-source electrical conflict.

## Residual hardware limit

A daemon cannot guarantee an SM5705 boost-off write succeeds after total loss
of its I2C bus. A kernel driver cannot make that guarantee either. Do not
present a successful off request as proof that VBUS is off. The real path must
verify readback and report failures. If testing cannot establish sufficiently
prompt shutdown and an unambiguous distinction between incoming and locally
generated VBUS, do not enable automatic sourcing under the owner's risk limit.
