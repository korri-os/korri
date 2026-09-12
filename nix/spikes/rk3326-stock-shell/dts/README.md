# R36T Max board device tree

Written to mainline conventions so it can be submitted to the Linux kernel,
and usable as-is if that never happens. The two goals do not conflict: the
strict version is the one we would keep anyway.

Why strict matters here. This board's device tree does not exist anywhere —
no vendor board DTS, no upstream sibling that matches. The vendor ships a
generic Rockchip EVB tree with board nodes grafted on. Whatever we write is
the first description of this hardware, and if it stays ours we maintain it
against every kernel release forever. If it lands upstream, it rides along
for free.

## What is settled

| Area | Source of truth |
|---|---|
| Panel timings | The display controller, read while running: `720x720p61`, `clk[50000]`, `H: 720 860 940 1080`, `V: 720 740 744 764` |
| Panel reset | `gpio3 RK_PB7`, active low |
| Panel 1.8 V rail | `gpio0 RK_PB5`, active high |
| Backlight | `pwm1`, 25000 ns period |
| Stick ring LEDs | `gpio2 RK_PA1`, active high |
| Wi-Fi reset | `gpio0 RK_PA2` — see the polarity note below |
| Wi-Fi host wake | `gpio0 RK_PA5` |
| SDIO clock | 25 MHz, measured; 50 MHz times out firmware reset with `-110` |
| Audio | RK817 codec, headphone detect `gpio2 RK_PC6` — identical to the RG351M |
| Console | UART5, because the stock `earlycon` is at `0xff178000` |

GPIO numbers were resolved from the vendor tree's phandles, not guessed:
`gpio0@ff040000`, `gpio2@ff260000`, `gpio3@ff270000`, `saradc@ff288000`,
`pwm@ff200010` (which is `pwm1`).

## One deliberate disagreement with the vendor

The vendor tree declares the Wi-Fi reset GPIO **active-high**. This file
says active-low.

That is not a transcription error. On a mainline-lineage kernel, the June
bring-up measured both: active-high stopped SDIO enumerating at all, with
`mmc2: Failed to initialize a non-removable card`, while active-low
enumerates, downloads firmware, and associates. `mmc-pwrseq-simple` asserts
the line differently from the vendor driver, so the vendor's polarity is
correct for the vendor's kernel and wrong for ours.

If SDIO ever fails to enumerate after a kernel bump, this is the first line
to suspect.

## What is deliberately missing

**Buttons and analog sticks.** The vendor driver takes nineteen GPIOs as one
ordered list with no labels and assigns key codes by position, inside a
`play_joystick` driver that does not exist upstream. The pins are known;
which pin is which button is not derivable from the device tree. Writing
`gpio-keys` and `adc-joystick` nodes requires pressing each button on the
hardware and watching which line moves.

Guessing here would produce a file that compiles, boots, and maps the wrong
buttons — the kind of error that survives review because it looks complete.

## What does not belong in this file

Mainline's ST7703 driver carries mode timings and init sequences in C, one
set per compatible, so the panel's 24-command bring-up sequence is a driver
change rather than a device tree property. This board needs its own
compatible there. The two registers that matter are `SETVCOM 0x97 0x97` and
`SETPOWER_EXT 0x26 0x22`; the RG353M needed `0x7f 0x7f` and `0x26 0x62`, and
getting them wrong makes the panel report ready and draw nothing.

`nix/devices/rg353m/st7703-panel-module.nix` already builds exactly that one
driver out of tree, without rebuilding a kernel.

## Before this could be submitted

`aislpc` is not a registered vendor prefix. Upstream has `anbernic`,
`gameforce`, `hardkernel`, and `powkiddy`, but nothing for this vendor, so a
submission needs a one-line patch to
`Documentation/devicetree/bindings/vendor-prefixes.yaml` first. The
compatible string `aislpc,r36t-max` is a guess at the right vendor: the
retail brand is AISLPC, the board silkscreen reads `RG42T`, and resellers
rebadge it. That is a question for the mailing list, not one to settle alone.

The file is written so that answer can change without anything else moving.
