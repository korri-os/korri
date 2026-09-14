# R36T Max hardware evidence

This records saved stock/ROCKNIX surveys and the NixOS bring-up. A device-tree
value is a software declaration, not an electrical measurement. The diagnostic
image has booted on the handheld; physical acceptance is still incomplete.
See [the autonomous live survey](HARDWARE-SURVEY-2026-09-14.md) for current
thermal, network, storage and driver observations.

Original artifacts remain outside Git at
`~/.local/share/korri/rk3326-stock-shell/artifacts/`. They include `rg42t.dtb`,
its decompiled `rg42t.dts`, kernel configuration and RK915 firmware. Raw
surveys remain in the sibling `logs/` directory. Device dates are wrong;
identify records by their contents and boot IDs.

## Stock identity

| Field | Saved observation |
|---|---|
| Hostname | `R36tMax`. |
| Stock OS | EmuELEC `4.7-Nexus_nightly_20260116`. |
| Stock kernel | `5.10.160-g8883965efde8-dirty`. |
| DT model | `Rockchip rk3326 evb lpddr3 v12 board for linux`. |
| DT compatible | `rockchip,rk3326-evb-lp3-v12-linux`, `rockchip,rk3326`. |
| Board DTB | `/flash/rg42t.dtb`. |
| Stock reported RAM | 993,980 kB. |
| NixOS console reported MemTotal | 1,005,508 kB. |

The generic EVB model string does not establish the fitted RAM technology or
every supply route. It is not a substitute for board-specific evidence.

## Console and boot

The stock command line names early UART address `0xff178000`, which is UART5
on PX30. Upstream `odroid-go2_defconfig` instead uses UART2 at `0xff160000`.
Those are source facts; neither proves an accessible external serial cable.
The UART pads are internal. Current NixOS uses `console=tty0`, without
`earlycon`.

The generic DSI driver produced the previous visible NixOS root console.
`bootloader/` now builds mainline U-Boot with Rockchip DDR code, but its
cold-start and warm-restart behavior is unverified. The preserved loader is
still the recovery baseline. Empty pstore records after power-off do not
establish warm-reset retention or a broken ramoops implementation.

## Panel

The saved stock DRM summary reports:

```text
Display mode: 720x720p61    clk[50000] flag[a]
H: 720 860 940 1080
V: 720 740 744 764
bus_format[100a]: RGB888_1X24
```

This corresponds to approximately 60.60 Hz. The stock framebuffer's 680 by
680 plane is not the panel resolution. The NixOS survey also reports DSI-1
connected and enabled at 720 by 720.

The vendor DTS declares four DSI lanes, RGB888, active-low GPIO3 PB7 reset,
and GPIO0 PB5 panel enable. Its 153 by 85 mm physical size is implausible for
this square panel and remains untrusted metadata.

The community overlay was derived from the same vendor DTB. It is not an
independent hardware measurement. A cached response does not prove that
another owner tested it. Its preferred timing matches the saved DRM timing.
It also lists eleven alternative modes; there is no evidence that the driver
automatically tries them all.

The selected generic driver uses flags `0xe03`. The vendor declares `0xa03`.
The candidate ST7703 entry differs again. Exact Linux 6.12.63 source inspection
explains packet dispatch and flag handling in `dts/PANEL-PMIC-AUDIT.md`.

All 22 vendor register payloads match the generic and ST7703 entries. Their
schedules do not match. Generic interprets `wait=250` and `wait=50` as
hexadecimal, yielding 592 and 80 ms, and sends two further tail commands.
Those differences do not identify the minimum fix. Keep the working generic
schedule until a measured comparison justifies a change.

## Wi-Fi

The stock firmware uses Rockchip RK915 over SDIO, not a Realtek driver.
The saved `rk915_fw.bin` and `rk915_patch.bin` match the pinned prior-proof
source. See `wifi/README.md` for hashes and distribution restrictions.

The legacy June proof records working association and SSH on Linux 6.12.79
with GPIO0 PA2 active-low reset, PA5 host wake, and 25 MHz SDIO. It records
50 MHz firmware-reset timeouts and failed enumeration with active-high reset.
The current 6.12.63 kernel/module/compiled DTB checks pass. The diagnostic boot
has working Wi-Fi and pinned SSH at 25 MHz SDIO. A five-minute low-rate probe
recorded 6.33% packet loss and multi-second delays; a subsequent one-minute
handheld/router control was clean. Reliability is not accepted. The existing
SDIO supply definitions differ from the proof and remain a physical validation
question.

## PMIC and storage

The vendor DTS declares RK817 and RK817 battery/codec functions. The Linux
MFD driver chooses its variant from that declaration, not a silicon ID read.
The declaration strongly supports RK817 but does not measure the chip.
RK809's `SWITCH_REG1/2` entries are not missing RK817 descriptors. The vendor
backlight has no supply property. Its electrical feed remains unresolved;
do not invent a control register or a regulator consumer to fill that gap.

| Controller | Function | Evidence |
|---|---|---|
| `ff370000` | SD card. | Vendor/ROCKNIX DTS and running MMC record agree. |
| `ff380000` | SDIO Wi-Fi. | Vendor node and RK915 proof agree. |
| `ff390000` | Internal eMMC. | Eight-bit non-removable vendor node. Disabled in our DTS. |

The diagnostic selector requires `NIXOS_BOOT` on the physical SD controller's
first partition. A missing label means no card log. It never scans unrelated
FAT filesystems for somewhere to write.

## USB, input and sound

USB reachability is unresolved. A saved survey reported a configured CDC
function at high speed; zero extcon values alone do not prove missing data
wires. The current PHY reports a dedicated charger while the UDC is not
attached. A working host USB data connection is still unverified.

The ROCKNIX stick implementation uses a GPIO-controlled analog multiplexer
feeding ADC channel 1, not four independent channels. The vendor DTS's four
logical axis labels do not contradict that physical multiplexing route. Game
buttons, sticks, speaker-amplifier control and rumble remain unintegrated.
The current kernel registers RK817 ALSA and headphone detection, but no audible
output or jack-change test has been performed. An existing shared InputPlumber
process is not proof that the handheld controls work.
