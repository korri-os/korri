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

## Game buttons

The board now declares 17 buttons through native `gpio-keys-polled`, with
10 ms polling and explicit GPIO-mode, plain pull-up pinctrl. It keeps the
native device identity and 5 ms debounce default. No custom joystick driver,
axes, volume, rumble or shared input-policy change is included. Polling adds
work while the input device is open; this is not a full joystick.

The GPIO/code map, labels, polling and pinctrl come from
[ROCKNIX's EE-clone DTS](https://github.com/ROCKNIX/distribution/blob/697d112a64e442e79e3d27b064351d917657301a/projects/ROCKNIX/devices/RK3326/linux/dts/rockchip/rk3326-gameconsole-eeclone.dts),
revision `697d112a64e442e79e3d27b064351d917657301a`, lines 166–265 and
819–841. Source labels are not owner-confirmed physical legends. Linux
6.12.63's `Documentation/devicetree/bindings/input/gpio-keys.yaml` and
`drivers/input/keyboard/gpio_keys_polled.c` define the native binding,
identity and debounce defaults. Use symbolic Linux codes, not the stale
numeric comments beside some ROCKNIX codes.

The saved vendor `~/.local/share/korri/rk3326-stock-shell/artifacts/rg42t.dts`
(SHA256 `f313a0e63b0804f204158fa45a1b7a73cbabdb2c171f87ae18dc55ca7b38b8cc`)
corroborates the GPIOs and active-low polarity, not their key codes. Lines
4506–4507 declare count 18 but contain 19 GPIO tuples; lines 4412–4414
also contain 19 pinctrl tuples. The 17 explicit ROCKNIX children exclude
GPIO3 PD2/PD3 and retain Fn on PB2. Do not truncate the list and lose Fn.
Vendor PC0–PC7 declare pull-up plus 12 mA drive strength; this slice uses
ROCKNIX's plain pull-ups for all 17 inputs. Motor GPIO3 PA6, panel-reset
GPIO3 PB7 and mux-selector GPIO2 PB7 are not button pins.

`check-game-buttons.py` reads a compiled DTB. It checks all 17 GPIO/code
pairs, active-low flags, native button-only properties, 10 ms polling,
exact pinctrl coverage and plain pull-ups. `game-buttons-check.nix` compiles
only the board DTB using host tools and the pinned Linux headers/DTS includes;
it also runs `test-game-buttons.py` with 14 malformed-DTB rejection cases and
the existing radio/eMMC DTB check. These mutations add host test time, not device
work. The existing
`nix run .#r36tmax-check` task includes this check through `module-check.nix`.
For only the focused check, without any kernel build:

```sh
nix build --impure --no-link --print-out-paths --expr '
  let flake = builtins.getFlake (toString ./.);
  in import ./nix/devices/r36tmax/dts/game-buttons-check.nix {
    pkgs = flake.inputs.nixpkgs.legacyPackages.x86_64-linux;
  }'
```

**Host checks prove binding composition, not physical buttons.** Hardware
acceptance still needs driver probe and evdev classification, each control's
press/release (including Fn and both stick clicks), held combinations,
bounce/stuck-key checks, and delivery into Korri through the existing host.
No device deployment or input-policy change is implied by a passing check.
Confirm that panel operation, Wi-Fi and quiet idle remain unchanged on the
acceptance boot. Do not treat registration alone as working controls.

## Exclusions and upstream work

Sticks, speaker-amplifier control and rumble remain outside this slice. The
working ROCKNIX stick driver uses an analog multiplexer on ADC channel 1.
Do not replace it with an assumed four-channel joystick description.

Upstream submission needs a working ST7703 implementation, validated supply
bindings, board identity/vendor-prefix review and binding checks. Do not
infer RK817 switch regulators from RK809 nodes copied into the vendor EVB
template. No register descriptor or voltage change is justified by that
name match.
