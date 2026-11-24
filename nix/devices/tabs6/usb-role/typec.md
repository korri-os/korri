# Tab S6 Type-C register grounding

## Current owner

The Korri product uses the read-only kernel `s2mm005-read-only` driver for
data-role policy and the standard Type-C class. See `typec-kernel.md` for
resource choices, interlocks, manual unbind/rebind and tests. The Phase A
observer below is historical diagnostic work; it is no longer a boot service
and cannot access 0x33 while the kernel driver is bound. All register and
firmware safety limits below still apply.

## Safety boundary

Phase A is an observer. It cannot switch role, enable boost, send commands,
acknowledge s2mm005 interrupts or load firmware. It has no write/configuration
CLI. Standard combined reads send only the register address before a repeated
START and read; they never send register data. No bus scanning and no forced
access to driver-owned addresses. Existing charge maintenance and boot-time
VBUS-off remain unchanged.

The s2mm005 firmware, flash and OTP are permanently out of scope. Samsung's
firmware implementation is not copied, compiled, called or installed. No PD
requests, power contracts, alternate modes or role-swap commands are sent.

All system/ESP updates use `tabs6-deploy-over-ssh` and `tabs6-esp` with boot
counting and LAST KNOWN GOOD preserved. No Odin, fastboot or partition tools.

## Grounding

Reference source: `qaz6750/kernel_samsung_sm8150_gts6lwifi` commit
`37cf30da0578a6378a3e3ef5e17aede89233defe`.

| Fact | Source checked |
| --- | --- |
| s2mm005 at 0x33, IRQ TLMM GPIO123 | Both DTBO entries extracted from `AP_T860XXS5DWH1_T860XXS5DWH1_MQB68901788_REV00_user_low_ship_MULTI_CERT_meta_OS12.tar.md5`, fragment 130 |
| Controller qupv3_se15, MMIO 0xc94000 | DTBO `__fixups__.qupv3_se15_i2c`; stock-boot.img embedded DT `__symbols__.qupv3_se15_i2c` |
| SDA/SCL GPIO27/28, function qup15, 2 mA, no bias | Stock DTBO qupv3_se15_s2mm005_i2c_active; mainline sm8150.dtsi qup_i2c15_default agrees |
| SM5705 charger 0x49, MUIC 0x25, controller 0x890000 (DT i2c4) | DTBO fragment 133, fixup qupv3_se4_i2c; stock boot symbols |
| MUIC IRQ GPIO122, charger IRQ GPIO41 | DTBO fragment 133 |
| Register addressing: two bytes, MSB first | drivers/ccic/s2mm005.c `s2mm005_read_byte` |
| FUNC_STATE 0x0020, four little-endian bytes | include/linux/ccic/s2mm005.h `FUNC_STATE_Type`; drivers/ccic/s2mm005_cc.c `process_cc_attach` |
| Interrupt status 0x0030, 32 bytes | s2mm005_cc.c `process_cc_get_int_status`; observer deliberately omits its interrupt-clear command |
| Low-power/water flags 0x0060, four bytes | s2mm005_cc.c `process_cc_water`, `LP_STATE_Type` |
| MUIC IRQ 0x03..05, ADC 0x09, device types 0x0a..0c, VBUS_VALID 0x15 bit0, charger type 0x17 | drivers/muic/universal/muic_regmap_sm5705.c enum and `sm5705_get_vps_data` |
| Charger STATUS1 0x08 (VBUSPOK bit0, VBUSUVLO bit1), CNTL 0x0c, boost 0x20 | include/linux/mfd/sm5705/sm5705.h; existing tabs6-usb-role and reference USB_HOST.md |

MUIC interrupt registers are read-to-clear latches. Phase A logs their returned
bytes, without configuring any masks. The s2mm005 interrupt is not cleared;
it may stay asserted, so periodic status reads remain necessary.

Linux adapter numbers are **not** DT controller numbers. Before this change,
Linux i2c-1 is DT i2c4, while Linux i2c-4 at 0xc80000 holds the touchscreen
at 0x49. Never infer a bus from an address alone or force access to it. The
observer resolves adapters by their `of_node` controller name, rejecting
missing or duplicate matches. GPIO chip lookup also uses its OF node, the
SM8150 TLMM at 0x3100000, not a gpiochip number.

## Phase A choice (superseded)

Use a small std-only Rust userspace observer, I2C_RDWR reads through i2ctransfer
and gpio-cdev edges through libgpiod. This avoids introducing a kernel driver
with an unproven register interpretation or porting any vendor probe path.
Only enabling the existing GENI I2C adapter requires a kernel change. Status
is logged every 250 ms plus IRQ edges; no Type-C policy runs in Phase A.

Cost: subprocess reads are not atomic across the two chips, and this observer
is not a production role-switch policy. Later policy must not use cached log data
as permission to power the port. It needs a fresh, consistent safety snapshot
immediately before each enable and fail-closed handling while power is on.

## FUNC_STATE decoding

Byte 0: PD state. Byte 1: CC1 bits 2:0, CC2 bits 6:4. Byte 2: next PD state.
Byte 3: attached bit0, source bit1, DFP bit2, Rp current bits4:3, short flags
bits6:5, reset bit7. These are logged as facts, not used to enable power.

## Phase A capture

Start from commit `68291af0d` (the tablet's LAST KNOWN GOOD). After deployment:

```
ssh -p 2222 root@192.168.1.119 \
  'journalctl -b -u tabs6-typec-status -f -o short-monotonic'
```

Keep the logger running before requesting any plug action. Ask readiness
before starting each timed sequence, one direct connection at a time: empty,
charger, empty, Kishi without pass-through power, empty, PC, empty. Include
charger-through-Kishi as a separate case, never as evidence for permission to
source power. Keep raw readings, transitions, errors, IRQ timestamps, gauge
state and charger mode. Phase B is blocked until attached sink, detached and
incoming-power states are unambiguous.
