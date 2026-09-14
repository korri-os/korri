# R36T Max device tree and kernel

`kernel-trimmed.nix` builds Linux 6.12 with the retained RK3326 configuration,
this board DTS, the working generic panel driver and the RK915 MMC quirks.
`config` derives from ROCKNIX's RK3326 configuration. The device integration
adds the nft-backed firewall features required by the pinned NixOS policy.

This is a working-reference port, not an upstream-ready device tree. In
particular, `rocknix,generic-dsi` and its `panel_description` are not upstream
bindings. The ST7703 candidate is compiled but not selected. Its matching
register bytes alone do not prove that its timing or lifecycle works.

Read `PANEL-PMIC-AUDIT.md` for the exact source comparison and the unresolved
power routes. `check-panel-pmic.py` checks the actual 6.12.63 source and vendor
DTS, including four negative controls. Generic's hexadecimal wait parsing
must not be changed while preserving the known-good display baseline.

## Grounding

| Area | Evidence |
|---|---|
| Panel timing | Saved DRM record: 50 MHz, H 720/860/940/1080, V 720/740/744/764. |
| Panel reset | Vendor GPIO3 PB7, active-low. |
| Panel enable | Vendor GPIO0 PB5, active-high. Actual input rail/voltage remain unmeasured. |
| Backlight | Vendor PWM1, 25,000 ns period, without a power-supply property. |
| Wi-Fi reset | June mainline proof requires GPIO0 PA2 active-low. Vendor's active-high declaration serves a different driver. |
| Wi-Fi host wake | Prior RK915 proof binds GPIO0 PA5 to SDIO function 1. |
| SDIO clock | Prior proof associates at 25 MHz; 50 MHz produced reset timeouts. |
| SD/eMMC | ff370000 is SD; ff390000 is eMMC and remains disabled. |
| Console | Stock early UART address ff178000 is UART5; current NixOS console is the panel. |

The compiled DTB check runs with each RK915 module build. It verifies the
radio clock, PA2 reset polarity, PA5 wake binding and disabled eMMC. It does
not measure those signals on the board.

## Exclusions and upstream work

Buttons, sticks, sound and rumble are not part of this slice. The working
ROCKNIX stick driver uses an analog multiplexer on ADC channel 1. Do not
replace it with an assumed four-channel joystick description.

Upstream submission needs a working ST7703 implementation, validated supply
bindings, board identity/vendor-prefix review and binding checks. Do not
infer RK817 switch regulators from RK809 nodes copied into the vendor EVB
template. No register descriptor or voltage change is justified by that
name match.
