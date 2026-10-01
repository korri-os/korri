# Return RG35XXSP on the product module with delivery

Status: ready-for-agent
Blocked by: 06, Phase 2 gate

## What to build

Turn RG35XXSP from a console bring-up image into a normal Korri product device with a published image and signed closure. Make the handheld's screen, buttons, Wi-Fi, and audio work before calling the port complete.

## Acceptance criteria

- [ ] RG35XXSP imports the product module and returns to `nixosConfigurations` with no product-check exception.
- [ ] Its device module supplies only observed hardware facts and explicit recorded limits. Unknown compositor or input values are measured on hardware rather than guessed.
- [ ] The ROCKNIX-derived Linux 7.2 display work from ticket 06 is used. Panfrost, mainline U-Boot v2025.10, AXP717, and RTL8821CS remain hardware facts.
- [ ] Automatic root login on serial and virtual consoles is removed from the product image.
- [ ] The built-in screen, physical buttons, Wi-Fi, and audio work on hardware through normal Korri use.
- [ ] Bluetooth, battery percentage, real-time clock, and USB host mode are either working or each has one explicit recorded-limit sentence.
- [ ] The 1 GB RAM size and 640x480 panel are recorded as hardware facts, not limits or blockers.
- [ ] The device declares no sleep state on day one unless verified work from ticket 17 provides one. Absence is recorded as a limit.
- [ ] The hardware encoder result is recorded. If no usable encoder exists, the streaming host is omitted from the default selection rather than disabled.
- [ ] The image workflow publishes an RG35XXSP installation image, and the cache workflow publishes its complete signed closure from the same commit.
- [ ] A fresh flash reaches the portal with no manual fixes and can browse and launch a local game with working controls and audio.
- [ ] The device and workflow checks cover hardware facts, product-module import, and delivery. They do not widen the shared check's exclusions.
- [ ] The image remains a development image until the full physical acceptance list in the canonical spec passes. Build, evaluation, and delivery alone do not grant support.
- [ ] No firmware partition, forbidden internal-storage partition, migration path, or compatibility fallback is added.

## Hardware gate, 2026-09-22

The ROCKNIX-derived Linux 7.2 kernel files are present under `nix/devices/rg35xxsp/kernel/`, but this repository has no observed DRM node, active panel connector, controller mapping, Wi-Fi association, or audio result from an RG35XXSP. No USB serial device appeared at `/dev/ttyACM*` or `/dev/ttyUSB*` on the current build host. Do not guess display or input facts to force product evaluation, or publish an image as a supported product without the physical acceptance pass.

## Progress, 2026-10-01

On the owner's request ("get the information from ROCKNIX, build them out,
flash new cards"), the RG35XX SP and a new RG35XX Pro import the product
module from a shared H700 family (`nix/devices/h700`), with image and cache
workflow entries. This does not pass the hardware gate above. The display,
input, audio and Wi-Fi values are read from the device trees and driver
sources, not guessed and not yet observed; each file names its source. The
first boot of each card must confirm or replace them.

Found while porting: patch 0211 binds the gamepad to `rocknix-singleadc-joypad`,
but nothing built that driver, so the earlier image had no buttons. The kernel
now builds it in tree from ROCKNIX's pinned source.
