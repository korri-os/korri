# Decide RG35XXSP product adoption

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: claimed
Blocked by: 07

## Question

RG35XXSP is in the device set and its console-only composition is a debugging state, not a hardware limit ([Decide the shared product and hardware boundary](07-decide-product-device-boundary.md#ownership)). Today [its device module](../../../nix/devices/rg35xxsp/default.nix) imports only `nix/base`, the device cache policy, and the SD format. Which hardware facts must it supply to the product module, which of its parts (H700 kernel, mainline U-Boot, Panfrost, AXP717, RTL8821CS) are bring-up that stays, and what is unknown about the hardware?

Inspect the [README](../../../nix/devices/rg35xxsp/README.md), [kernel.nix](../../../nix/devices/rg35xxsp/kernel.nix), [uboot.nix](../../../nix/devices/rg35xxsp/uboot.nix), and [module-check.nix](../../../nix/devices/rg35xxsp/module-check.nix) for what is verified and what is assumed. Compare with the closest already-integrated device for the hardware facts the product module will require. Do not decide the product module's option list here; that is extracted from the existing host and portal modules during `/to-spec`.

Resolve with Simon what RG35XXSP must provide, which of its 1 GB RAM and 640x480 display constraints are recorded limits versus required-behavior blockers under [Define hardware limits and product acceptance](02-define-device-support.md#answer), and whether it has any delivery route before the image and cache workflows cover it.
