# Out-of-tree drivers carried into the kernel build

## panel-generic-dsi.c

ROCKNIX's data-driven DSI panel driver, verbatim from
`projects/ROCKNIX/packages/linux-drivers/generic-dsi/sources/` in the ROCKNIX
distribution (upstream: <https://github.com/stolen/overlay_server>, author
Danil Zagoskin, GPL-2.0). One edit: `<linux/hex.h>` is newer than 6.12, so
`hex2bin` comes from `<linux/kernel.h>` here.

It reads the whole panel description from the device tree's
`panel_description` strings: size, delays, lanes, format, DSI flags, the
init sequence as DCS writes, and one or more display modes. The community
generator at <https://rocknix.gosk.in/dtbo/> produces that description from
a unit's stock dtb, and `fetch-panel-overlay` fetches it; so the panel node
for this board is the generator's output, not a hand transcription.

Why it is here: seven boots of mainline's ST7703 driver with this glass's
own sequence, timings and phase left the panel white behind a lit
backlight, while the SoC side reported success at every step. This driver
draws on this exact unit under ROCKNIX. Booting it under our kernel
separates "our kernel cannot drive this DSI link" from "our ST7703 entry
is wrong", and a working screen makes the second question cheap.

It is not the destination. Once the screen works, the differences between
what this driver sends and what the ST7703 entry sends are read off with
both in hand, and the ST7703 entry becomes right and upstreamable.
