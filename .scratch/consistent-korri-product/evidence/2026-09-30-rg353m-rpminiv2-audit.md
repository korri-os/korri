# RG353M and RP Mini V2 product audit, 2026-09-30

Scope: the RG353M (`nixosConfigurations.rg353m`) and the RP Mini V2
(`nixosConfigurations.rpminiv2`) at commit `9ba090fa1`. The other devices are
out of scope.

Method:

1. Evaluate the effective NixOS configuration of both systems. Remove store
   hashes, then compare the final systemd unit text, user units, tmpfiles,
   udev rules, polkit rules, `/etc`, packages, users, boot, network and the
   Korri options. Scripts: `/tmp/audit-353-mini/{dump.nix,eval.sh,compare.py}`
   (temporary).
2. Read the RG353M's journal from its card (boot `5f9854b8…`, after the bundle
   fix). Read `/var/lib/korri` on the card, with the block device read-only.
3. Compare the differences with the product spec
   (`.scratch/consistent-korri-product/spec.md`).

"Verified" means the evaluation, the code or the RG353M journal shows it.
"Inferred" means the evidence supports it, but no test proved it.

## Root cause

The Mini became the reference device. New product behavior went into Mini
files. When the product check rejected Mini-only behavior, the check got a
Mini-only exception, and the behavior did not move into the product module:

- `nix/product/reference.nix` has `deviceReferences.rpminiv2`. It gives the
  Mini its own trusted product reference.
- `nix/devices/rpminiv2/product-reference.nix` adds the patched systemd, the
  portal freeze during games and the audio socket access for inputd. Commit
  `6236a0d8e` (2026-09-26) added it.
- No device other than the Mini has such a reference. So the product check
  accepts an RG353M without these behaviors.

The product check compares a fixed list of 49 settings
(`nix/product/requirements.nix`) and the product units. It does not see
networkd, tmpfiles ACLs, inputd actions, the boot splash or timing budgets.

## Product behavior that only the Mini has

| # | Behavior | Mini source | RG353M state | Class |
|---|---|---|---|---|
| M1 | Portal freezes while a game runs, and thaws after | `clients/portal/nix/kiosk-freezer.nix` + patched systemd `nix/devices/rpminiv2/systemd/`, imported in `portal.nix:32` | Absent (verified: no `KORRID_PORTAL_UNIT`, no freezer polkit rule) | Product behavior. Needs a decision, because it needs the patched systemd. |
| M2 | Volume buttons change the volume | `portal.nix:78` inputd `volume-up`/`volume-down` actions, plus the inputd PipeWire socket bind | Absent (verified) | Product behavior. The command (`wpctl`) is not board-specific. |
| M3 | Controller activity keeps the screen awake, and the display goes idle | inputd `controller-activity` action, `rpminiv2-display-idle.service` | Absent (verified) | Product behavior. The idle policy can depend on the panel. |
| M4 | The game user can read the library and write saves | `nix/devices/rpminiv2/game-plugins.nix` tmpfiles ACLs on `/var/lib/korri`, `roms/*`, `users/default/*` | Absent. On the card, `/var/lib/korri` is `drwx------ korrid:korrid` with no ACL and no `roms` or `users` directory (verified). Games that run as `korri` probably cannot read ROMs or write saves (inferred). | Product behavior |
| M5 | No 120 s network wait at boot | `usb-gadget.nix:61` `wait-online.enable = false` | On. It fails after 120 s on every boot (verified), which starts the chain that blocks all input | Product behavior |
| M6 | CPU and GPU clock governor | `services.korri.clockGovernor` (`nix/base/clock-governor.nix`) | Uses `powerManagement.cpuFreqGovernor = "schedutil"` instead, with no GPU governor | Same intent, two mechanisms |
| M7 | SSH plugin preinstalled | `nix/product/plugin-selection.nix` | Not selected | The spec says SSH is "not preinstalled". So the Mini deviates from the spec. This deviation also leaves the RG353M with no remote access. |

## Product behavior that only the RG353M has

| # | Behavior | RG353M source | Mini state | Class |
|---|---|---|---|---|
| R1 | Boot splash (Plymouth and `korri-boot-splash-handoff`) | `nix/devices/rg353m/sd-image.nix:15,31` | Absent. The Mini shows boot text, then a shell, before Shift. | Product behavior |
| R2 | Verbose development boot: `loglevel=7`, serial console `ttyS2` (its getty fails), emergency and rescue shells, `cpupower`, `libva-utils`, `v4l-utils`, `nixos-install` | `nix/devices/rg353m/` | The Mini boots `quiet` at `loglevel=4`, with no emergency shell | Development leftovers in a product image |
| R3 | `root` in group `video` | RG353M modules | Not in the group | Probably a leftover |

## Same configuration, different result at runtime (RG353M journal)

| # | Fault | Evidence | Cause |
|---|---|---|---|
| T1 | The seat receiver fails 3 times, then hits the start limit. korrid and the kiosk then fail 23 times on the dependency, and `korrid-control.socket` hits its trigger limit. | `Korri seat event node did not reach the required group and mode` at 18 s, 21 s and 24 s | `wait_for_event_node` in `services/inputd/src/input_seat_uinput.rs:127` waits a fixed 2 s (100 × 20 ms). That budget holds on the Mini. On the RG353M, udev is slower at boot. |
| T2 | All controller input is fenced | Plugin host fails after the 120 s wait (M5). korrid then cannot read the plugin list. | korrid reads the seat count through `config::settings::read`, which also reads the plugin registry |
| T3 | USB gadget dead | `dwc3 fcc00000.usb: failed to enable ep0out` | RG353M kernel 6.18.2. Not investigated. |
| T4 | First-boot plugin proof takes 134 s | Journal | Same as on the Mini (96 s), but slower |

## Hardware facts (differences allowed by the spec)

Display mode and DRM path, compositor transform and touch mapping, kernel
version and modules, USB gadget script, ALSA UCM (Mini), `rpminiv2-audio-boot`
and `rpminiv2-va-macro-retry`, InputPlumber device data, hostname and USB
network address, and NetworkManager Wi-Fi powersave on the RTL8821CS. The
spec allows the DS emulator on the Mini only.

## Already fixed today

- Each bundle starts its own device InputPlumber data (`31f579420`, `9ba090fa1`),
  with an assertion.
- The 1 Hz inputd reconcile no longer stalls pad reads (`93a77b209`).
- The RetroArch seat reservation (`korri-plugins` `0cbdfdd`). It is not in a
  signed publish yet.

## Spec deviations found on both devices

- The spec says Moonlight ships preinstalled. Neither selection contains it.

## Decisions needed

1. M1: should the portal freeze ship on every device? That requires the
   patched systemd on every device.
2. M7: should SSH be preinstalled on every device, or on none, as the spec
   says?
3. R1 and R2: should the boot splash and quiet boot be product behavior for
   every device?

Items M2 to M6 and T1 to T2 fall under existing product decisions. Each one
needs the behavior in the product module plus a product check assertion.
