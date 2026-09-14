# R36T Max panel / PMIC source audit — steps 5 and 6

**Keep `rocknix,generic-dsi` and its current schedule.** All 22 register
payloads match the vendor and ST7703 candidate. This is source verification,
not proof that this kernel draws, that the candidate works, or of PMIC silicon
identity. This slice changes comments only in existing sources.

**Corrections to the earlier research:** Linux 6.12.63's RK809 descriptors use
SWITCH_REG1 → **vcc9**, SWITCH_REG2 → **vcc8**. The earlier binding-search
summary reversed those inputs. The actual DW host selects **burst before
sync-pulse**, so adding the sync-pulse flag does not select pulse mode here.
The vendor backlight has **no power-supply**; SWITCH_REG2 is not a proven feed.

## Evidence and reproduction

Audited against worktree base `a5955c0b` and the supplied Linux **6.12.63**
extracts in `/tmp/r36tmax-linux-source/`. `check-panel-pmic.py` pins SHA256 for
all seven Linux files below and applies the panel patch in memory, without
fuzz. The version attribution comes from the supplied extraction; this audit
did not download a release or inspect a complete kernel build tree.

| Key | Source actually inspected; anchors |
|---|---|
| V | Vendor `rg42t.dts`: `panel@0` lines 2779–2815; PMIC 1525–1840; backlight 4538–4544; `vcc18-lcd-n` 4619–4631; GPIO providers 3317–3363 |
| D | `rk3326-aislpc-r36t-max.dts`: panel description, backlight, fixed regulator, PMIC |
| G | `drivers/panel-generic-dsi.c`: `load_globals`, `load_init_seq`, `load_panel_description`, prepare/unprepare/probe/shutdown |
| P | `panel-st7703-r36t-max.patch`, reconstructed against S |
| S | `drivers/gpu/drm/panel/panel-sitronix-st7703.c`: command defines 29–54; lifecycle 676–753; probe 849–900 |
| C/H | `drivers/gpu/drm/drm_mipi_dsi.c`: transfer 444–456, DCS buffer/multi 885–969; `include/drm/drm_mipi_dsi.h`: flags 118–140, multi macros 295–305 and 449–453 |
| W | `drivers/gpu/drm/bridge/synopsys/dw-mipi-dsi.c`: message configuration 373–401, transfer 500–537, mode configuration 597–647, teardown/startup 936–1055 |
| M | `drivers/mfd/rk8xx-i2c.c`: `rk817_data`, `rk8xx_i2c_probe`, `rk8xx_i2c_of_match` |
| R/H | `drivers/regulator/rk808-regulator.c`: descriptor macros, `rk809_reg`, `rk817_reg`, probe; `include/linux/mfd/rk808.h`: regulator IDs and register defines |

V was read only from
`/home/simonwjackson/.local/share/korri/rk3326-stock-shell/artifacts/rg42t.dts`.
Its SHA256 is
`f313a0e63b0804f204158fa45a1b7a73cbabdb2c171f87ae18dc55ca7b38b8cc`.
No profiles, secrets, targets, raw disks or register interfaces were accessed.

Run from this directory, with the supplied local sources:

```sh
./check-panel-pmic.py --linux-source /tmp/r36tmax-linux-source \
  --vendor-dts /home/simonwjackson/.local/share/korri/rk3326-stock-shell/artifacts/rg42t.dts \
  --self-test
```

The Nix-shebang script reads files and prints a ledger. With an existing Nix
Python interpreter, `python3 check-panel-pmic.py` takes the same arguments
without resolving a shell. It performs static source checks, not execution of
Linux C or emulation of the DSI host. Four in-memory negative controls change
a payload, packet type, wait radix and base command constant; all must fail.
No whole-kernel build is needed.

## Step 5: packets and callback timeline

Automated equality covers command order and every payload byte, resolving P's
symbolic command constants in exact S. V has 24 commands: 22 register writes,
then `11` and `29`. G's description has the same 24 payloads. P's register
function contains those same 22 register writes.

| Register commands | Packet type (hex); length includes command |
|---|---|
| B4 (2), BC (2), CC (2) | 15, DCS short write with parameter |
| B9 (4), B1 (6), B2 (4), B3 (11), B5 (3), B6 (3), B8 (5), BA (28), BF (4), C0 (10), C1 (13), C6 (7), C7 (7), C8 (5), E0 (35), E3 (15), E9 (64), EA (62), EF (4) | 39, DCS long write |
| 11 (1), 29 (1) | 05, DCS short write |

G's `I seq` uses **`mipi_dsi_dcs_write_buffer`**, not generic-write packets.
P's `mipi_dsi_dcs_write_seq_multi` constructs a byte array and calls
`mipi_dsi_dcs_write_buffer_multi`, then the same buffer helper. C selects
DCS SHORT_WRITE for length 1, SHORT_WRITE_PARAM for length 2, LONG_WRITE
otherwise. The numeric types above are the vendor stream's DSI types; the
supplied extract lacks `include/video/mipi_display.h`'s numeric definitions.

Both paths retain `MIPI_DSI_MODE_LPM`. C adds `MIPI_DSI_MSG_USE_LPM` before
host transfer. W creates the packet, sets `CMD_MODE_ALL_LP` and
`ENABLE_LOW_POWER_CMD`, then writes the packet. **Host command/video mode is
not the same setting as per-message LP/HS.** A completed transfer does not
prove panel acceptance or pixels.

| Stage, successful path | Selected G | Unselected P + exact S |
|---|---|---|
| Probe reset | Logical 0 | Logical 0 |
| Prepare power/reset | Enable vdd, then iovcc; wait 20; reset 1; wait 20; reset 0; wait 20 ms | Reset 1; enable iovcc, then vcc; wait 10–20; reset 0; wait 15–20 ms |
| Initial sleep-out | None | `11`; 250 ms |
| Registers | 22 writes | Same 22 writes |
| Following commands | `11`; **592 ms**; `29`; **80 ms**; extra `29`; extra `11`; **120 ms** | Common `st7703_init` sends `11`; **120 ms**; `29`; descriptor delay **120 ms** |
| Panel enable callback | No driver callback | Returns 0 (`init_in_prepare = true`) |
| Disable/unprepare | Unprepare sends best-effort `28`, `10`; asserts reset; disables iovcc/vdd | Disable sends `28`, `10`, waits 120 ms through multi context; unprepare asserts reset and disables iovcc/vcc |

G sends **26** writes, P **25**, V describes **24**. G's explicit delays after
supply enable total **852 ms**; P's reset and command waits total **515–530
ms**, excluding transfers, scheduling and regulator work. V's stream waits
are decimal 250/50 ms (`fa`/`32` bytes). G parses I-line waits with base 16:
`250` → 592, `50` → 80. G-line delays use integer-list parsing; the fifth
value is `ready`, whose sleep is commented out. V's separate lifecycle delay
properties cannot establish actual vendor callback timing without its driver.
V's exit stream is `05 00 01 28 05 00 01 10`; its 50 ms wait is in **init**.

W's pre-enable callback initializes the host, waits two frames, and selects
command mode for panel prepare. W's enable selects video for panel enable.
These are the exact host callback bodies and their documented panel ordering,
not an observed boot trace. The supplied extracts do **not** include the DRM
panel-bridge/core traversal implementation. Do not claim independently verified
whole-chain ordering, especially at teardown: W's post-disable body selects
command mode **and powers down**; a comment alone does not prove when G's
unprepare transfers run. G's own shutdown explicitly calls unprepare before
disable. S has remove/detach but no shutdown callback.

| Flags, decoded with exact H | G `0xe03` | P `0xa07` | V `0xa03` under H definitions |
|---|---|---|---|
| VIDEO, BURST, NO_EOT_PACKET, LPM | Yes | Yes | Yes |
| SYNC_PULSE (bit 2) | No | Yes | No |
| CLOCK_NON_CONTINUOUS (bit 10) | Yes | No | No |

W prioritizes BURST over SYNC_PULSE. Both select burst; the flag difference
is not evidence of a sync-pulse experiment on the wire. W sets
`AUTO_CLKLANE_CTRL` only for G's non-continuous flag and clears `EOTP_TX_EN`
for both. Historical vendor header/driver behavior is not supplied.

Reset resolves in V through `0x9f` to GPIO3 PB7, active-low: logical 1 is
physical low, logical 0 high. Panel supply `0x9e` resolves to the fixed
GPIO0 PB5 active-high regulator (`0x5c`, pin 13). D binds **both** G supplies
`vdd`/`iovcc` to that same producer; these are not two independent rails.
S requires **`vcc`/`iovcc`**. A compatible-only change leaves the `vcc` binding
wrong. V declares neither voltage nor input for the fixed rail; D's 1.8 V
constraint and `vin-supply = vcc_3v0` remain unverified and unchanged.

Preferred signal timing matches V: 50 MHz, H 720/860/940/1080,
V 720/740/744/764, negative sync. G declares 12 modes and reverses mode list
insertion; P declares one. G/V size is 153 × 85 mm; P is 76 × 76 mm.

**Unfixed source defects / selection blockers:** G ignores failures in its
24-command description loop; only its two extra tail writes trigger unwind.
P's added prepare-time init returns failure **without asserting reset or
releasing enabled supplies**. Exact S proves this path. No candidate behavior
was changed in this audit; it remains unselected and hardware-unverified.
Correct error cleanup before selecting it. Neither that repair nor source
comparison identifies the necessary minimum timing or fixes a white panel.

## Step 6: variant selection is not silicon identification

M obtains `rk817_data` through `device_get_match_data`, initializes the regmap,
and passes **`data->variant`** to `rk8xx_probe`. `rk817_data.variant = RK817_ID`
comes from OF `rockchip,rk817`, **not an ID-register read**. R selects the
regulator array using `rk808->variant`. A driver/variant name in a log is not
hardware-derived identity evidence. No chip marking or saved ID measurement
was inspected; the supplied extracts also omit the shared MFD core body.

| Exact 6.12.63 descriptors | Input | Source control fields (not measured state) |
|---|---|---|
| RK817 BOOST | vcc8 | Voltage 0xde mask 0x07; enable 0xb4 mask/value 0x22, disable 0x20 |
| RK817 OTG_SWITCH | vcc9 | Enable 0xb4 mask/value 0x44, disable 0x40 |
| RK809 DCDC_REG5 | vcc9 | Enable 0xb4 mask/value 0x22, disable 0x20 |
| RK809 SWITCH_REG1 | vcc9 | Enable 0xb4 mask/value 0x44, disable 0x40 |
| RK809 SWITCH_REG2 | vcc8 | Enable 0xb4 mask/value 0x88, disable 0x80 |

These values resolve R's `ENABLE_MASK(id)`, `DISABLE_VAL(id)` and H's
`RK817_POWER_EN_REG(3)`. They are driver declarations, **not** datasheet
reset defaults or permission to write registers. RK817 has four bucks,
nine LDOs, BOOST and OTG_SWITCH (15 descriptors); RK809 has five bucks,
nine LDOs and two switches (16). The overlapping IDs/register fields explain
why copying RK809 switches into RK817 is unsafe, not a missing-driver fix.

V declares DCDC_REG5, SWITCH_REG1/2 **and** BOOST/OTG_SWITCH. Mainline's RK817
array has no descriptors for the first three. Shared-template residue is an
**inference**; exact vendor regulator registration code was not supplied.

| Board source route | Verified declaration; limit |
|---|---|
| Backlight | PWM1 (`0xc7`), channel 0, period 25,000 ns; no power-supply. Electrical feed unknown |
| Panel | GPIO0 PB5 fixed rail; input and output voltage not declared in V |
| V PMIC vcc9 | `0x65` → BOOST; consistent with BOOST feeding OTG_SWITCH, not display power |
| V PMIC vcc7 | `0x4b` → DCDC_REG4; D instead uses vccsys |
| V BOOST voltage | `"", "I>"` encodes `00 49 3e 00` = 4,800,000 µV, not an absent value |
| R BOOST voltage | Linear 4.7–5.4 V, 0.1 V steps; 4.8 V is supported. D keeps 4.9–5.4 V |

## Closure and cost

The supplied-source comparison for steps 5/6 is recorded and repeatable.
**Hardware acceptance remains blocked.** No build, boot, deploy, register read,
or physical measurement was performed. Reports that ROCKNIX draws and prior
ST7703 boots stayed white were inherited, not independently observed here.

Keep the current selection and timing. Cost: the out-of-tree driver, ignored
init errors and unresolved backlight power ownership remain. This does not
prove suspend/resume or survival of a changed bootloader power state. Before a
candidate deployment, obtain the missing exact bridge/vendor driver evidence,
repair candidate failure unwind, and verify supply binding and an actual
picture on hardware. Silicon identity and electrical power routes need a
trusted saved measurement, chip marking, schematic or board-trace evidence;
software declarations cannot close those questions.
