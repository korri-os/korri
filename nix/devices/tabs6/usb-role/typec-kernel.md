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
| Gauge Charging | fresh `POWER_SUPPLY_PROP_STATUS == POWER_SUPPLY_STATUS_CHARGING` |
| Charger CNTL mode 5 | `(sample.cntl & 7) == 5` |
| Three fresh successful reads | saturating counter in `s2mm005-policy.h` |

Any nonqualifying sample or read error selects device and resets the counter
on the same work invocation. Only enables wait. A normal PC/UFP never qualifies.
Role-set errors reset qualification and force a device request. Probe, remove,
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
| Gauge `&sm5705_fg` on i2c11, 0x71 | Existing DTS and `sm5705_fuelgauge.c`. `power-supplies` phandle uses Linux 7.3's `power_supply_get_by_reference`, the successor to `power_supply_get_by_phandle`. STATUS reads fresh current (and SOC when needed), not the five-second notification cache. The existing gauge owns its current decode and 50 mA deadband. |
| dwc3 | Standard reciprocal OF graph endpoints link the controller to `&usb_1_dwc3`; `usb_role_switch_get` defers until the switch exists. Its default is peripheral. |

The charger and MUIC addresses are deliberately **not** reserved with dummy
clients. They have no kernel drivers. Adapter-locked combined reads coexist
with the unchanged charge-maintenance and boot boost-off i2c-dev services,
which would otherwise fail with EBUSY. No forced I2C access is used. This board
specific driver must be revisited if either address gains a kernel owner.
Cross-chip reads are sequential, as before; they are not a hardware-atomic
snapshot. Every poll gets new facts, and no observer log or gauge notification
cache grants permission.

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
host role it stops the gadget with `systemctl --no-block`, which cancels its
late-UDC wait and avoids delaying network startup for 60 seconds. On return to
device it restarts the gadget. This keeps the shared product gadget unchanged;
only the Tab S6 edge knows about its role switch. No role or chip write is part
of either gadget lifecycle action.

The existing boot `boot-vbus` service still requests verified boost-off because
SM5705 registers can survive warm reboot. The product has no user-space
`boot-role` writer; the driver owns boot data role. The console retains its
old manual boot-role service. Nothing here changes charge-current maintenance.

## Verification

`tabs6-typec-policy` compiles the exact header included by the kernel driver.
It replays all four real traces (Phase A, charging, sourcing flap and bounded
shutdown), exhausts all PD states/flag
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
