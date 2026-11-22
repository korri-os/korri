# Phase B design (not deployed; Phase A gate remains closed)

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
