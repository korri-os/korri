# R36T Max hardware facts

Read off the running device over SSH, not from vendor listings or community
posts. Every line here is either quoted from the device or derived from its
own device tree. Where earlier desk research disagreed, the device wins and
the disagreement is recorded, because those guesses were about to be built
against.

Artifacts live outside the repository, in
`~/.local/share/korri/rk3326-stock-shell/artifacts/`: the vendor device tree
(`rg42t.dtb`, decompiled to `rg42t.dts`), the RK915 Wi-Fi firmware, and the
running kernel config. They are vendor binaries and are deliberately not
committed.

## Identity

| Field | Value |
|---|---|
| Hostname | `R36tMax` |
| Stock OS | EmuELEC `4.7-Nexus_nightly_20260116` |
| Kernel | `5.10.160-g8883965efde8-dirty` |
| Model | `Rockchip rk3326 evb lpddr3 v12 board for linux` |
| Compatible | `rockchip,rk3326-evb-lp3-v12-linux`, `rockchip,rk3326` |
| Board DTB | `/flash/rg42t.dtb` — matches the `RG42T` silkscreen |
| RAM | 993,980 kB |
| CPU | 4 cores, `fp asimd evtstrm aes pmull sha1 sha2 crc32 cpuid` |

The device tree is a generic Rockchip EVB with board nodes grafted on. There
is no vendor board DTS to inherit from, so a mainline board device tree has
to be assembled from the nodes below.

## Console is UART5, not UART2

```
earlycon=uart8250,mmio32,0xff178000
console=ttyFIQ0 console=tty0 fbcon=rotate:3
```

On PX30, `0xff178000` is **UART5**. This matters: `odroid-go2_defconfig`,
which `nix/spikes/rk3326-boot-chain` builds from, sets
`CONFIG_DEBUG_UART_BASE=0xFF160000` — UART2. Built as-is, our U-Boot would
print to a port this board does not use, and the TPL console that the boot
chain spike worked to keep would be silent.

This also explains ROCKNIX retuning its DDR blob for "UART5 used on K36
clones". Same board family, same wiring.

## Panel: 720x720, Sitronix ST7703

Confirmed from the display controller while running, which is the only
source that settles it:

```
Display mode: 720x720p61    clk[50000] flag[a]
H: 720 860 940 1080
V: 720 740 744 764
bus_format[100a]: RGB888_1X24
```

Derived timings: pixel clock 50 MHz; horizontal front porch 140, sync 80,
back porch 140; vertical front porch 20, sync 4, back porch 20. These match
`timing0` in the vendor device tree exactly.

**`/sys/class/graphics/fb0` is misleading.** It reads `680,680`, and the DRM
connector lists no modes at all. The 680 is the *plane*, not the panel: the
stock OS draws a 680x680 image inset at 20,20 inside the 720x720 display.
Reading fb0 alone produces a wrong answer; read
`/sys/kernel/debug/dri/0/summary`.

Panel node: `compatible = "sitronix,st7703", "simple-panel-dsi"`, 4 DSI
lanes, `dsi,format = 0`, `dsi,flags = 0xa03`, reset GPIO active-low.
`width-mm = 153`, `height-mm = 85` — implausible for a square panel and
probably vendor boilerplate; do not trust it.

### Init sequence

`decode-panel-init.py` unpacks the vendor `panel-init-sequence` blob into 24
DCS commands, 370 bytes, fully parsed. Two of them decide whether this panel
draws anything:

| Register | R36T Max | RG353M (`patches/st7703-rg353v2-init-sequence.patch`) |
|---|---|---|
| `SETVCOM` (0xB6) | `97 97` | `7f 7f` |
| `SETPOWER_EXT` (0xB8) | `26 22` | `26 62` |

The RG353M needed an out-of-tree patch because mainline's values made its
glass report ready and draw nothing. This board's glass wants different
values again, so expect the same shape of work — and `nix/devices/rg353m/
st7703-panel-module.nix` already shows how to build just that one driver out
of tree instead of rebuilding a kernel.

## Wi-Fi: RK915, no mainline driver

`rk915` is loaded, with `rk915_fw.bin` and `rk915_patch.bin` from
`/lib/firmware`. Both are saved to the artifact directory. There is no
mainline RK915 driver, so this is out-of-tree work for as long as the device
is supported.

The SDIO settings proven on hardware in June still apply: reset on
`gpio0 RK_PA2` **active-low** (active-high broke enumeration outright), host
wake on `RK_PA5`, and `max-frequency = 25000000` — 50 MHz timed out firmware
reset with `-110`.

## PMIC and storage

`rk808 0-0020` with `rk817-battery`, so an RK817 driven by the rk808 driver
family. `rk817-battery: energy_mode missing!` is a stock warning, not a
fault.

Three controllers: `ff370000.dwmmc` (eMMC), `ff380000.dwmmc` (SDIO/Wi-Fi),
`ff390000.dwmmc` (SD card).

| Mount | Device | Type |
|---|---|---|
| `/flash` | `mmcblk0p3` | vfat, read-only |
| `/storage` | `mmcblk0p5` | ext4 |
| `/var/media/EEROMS` | `mmcblk0p6` | ext4 |
| `/storage/roms` | `mmcblk1p1` | vfat — the SD card |

Internal eMMC totals about 3.7 GB, which is smaller than the 7.28 GiB
recorded in May. The SD card is data-only and the boot ROM prefers it, so
an SD image takes priority while the stock system on eMMC stays the recovery
path.

`/storage` is `drwxr-xr-x root root`. That matters because Dropbear rejects
an `authorized_keys` directory whose parent is writable by others, which is
what broke the first payload staged under `/tmp`.
