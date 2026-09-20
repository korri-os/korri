# Decide device sleep tiers

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: resolved
Blocked by: 07

## Question

Device sleep arrives in tiers. Some devices have no deep sleep yet; later tiers may include ROCKNIX-style workarounds and hibernation ([Decide the shared product and hardware boundary](07-decide-product-device-boundary.md#sleep)). [Decide what happens when leaving and returning to a game](09-decide-live-session-behavior.md#leave-by-device-sleep) already fixes the behavior once sleep exists: freeze the exact launch before suspend, thaw on wake only if the user was in the game. What are the tiers, which tier does each of the six devices sit in today, and does a device with no sleep qualify as a supported image?

Inspect what each device declares now: [Odin disables sleep, suspend, hibernate, and hybrid-sleep](../../../nix/devices/odin2portal/platform-policy.nix), the RP Mini V2 OLED idle service in [portal.nix](../../../nix/devices/rpminiv2/portal.nix), and `PowerSuspend` in [inputd's action catalog](../../../services/inputd/src/action_catalog.rs), which has no trigger on any device. Check ROCKNIX's published behavior for the same boards before naming a tier after it; do not assume a workaround works on mainline kernels.

Resolve with Simon the tier names and what each promises to the user, whether tier "none" blocks supported status under [Define hardware limits and product acceptance](02-define-device-support.md#answer) or is a recorded limit, what the power button does on a device with no sleep, and what evidence proves a tier on hardware. Record decisions, not kernel work.

## Answer

Resolved 2026-09-20. All choices are Simon's, selected through the ask tool.

### The model

A device declares each sleep state it has. It does not hold one rank. Korri declares the states in Nix as a [hardware fact](../../../CONTEXT.md) and raises them by hand when kernel support lands. The product keeps one preference order and picks from what the device declares.

This replaces the ranked model in the glossary, which said a device sits in exactly one tier from none to hibernation. Hibernation does not need the firmware suspend path that deep sleep needs, so a device can hibernate while its deep sleep stays broken. One rank cannot state that. Resolved tickets keep their wording: where [the physical acceptance set](07-decide-product-device-boundary.md#physical-acceptance-set) says "if the device's sleep tier supports it", read "if the device declares a sleep state".

The option name and its location in Nix are not chosen here. Ticket 07 extracts the product module's option list during `/to-spec`.

### The states

- **Light sleep**: the screen is off, the live session is frozen, and the device stays awake. A fixed delay then shuts the device down cleanly, as ROCKNIX does. The delay is one product-wide value. This answer does not choose the number; ROCKNIX's reference default is 900 s.
- **Deep sleep**: suspend to RAM. The frozen live session returns on wake, with no time limit. This is the state [ticket 09](09-decide-live-session-behavior.md#leave-by-device-sleep) describes.
- **Hibernation**: a memory image on storage, then power off. **Deferred.** The state keeps its name so a device can declare it later. No device declares it and nobody implements it now.
- **No state**: the device has no sleep. This is a recorded limit, not a blocker.

Korri does not use the word `standby`. The kernel already uses it for power-on suspend, which is not the light-sleep behavior.

While hibernation stays deferred, the preference order is: deep sleep if declared, then light sleep if declared, then a clean shutdown.

### Physical controls

One press of the power button enters the device's preferred state. A second press returns, and [ticket 09](09-decide-live-session-behavior.md#return) owns what the owner sees on return. Closing the lid does what the press does. Opening the lid does what the second press does.

On a device that declares no sleep state, the power button and the lid both shut the device down cleanly, with no prompt. This replaces the ignored power key that Odin declares today. Korri does not restore a live session after a shutdown, so the session ends there.

### Support

Device sleep is an optional hardware feature. A device that declares no sleep state is still a [supported image](../../../CONTEXT.md) and records the absence as a [recorded limit](../../../CONTEXT.md). Under [ticket 02](02-define-device-support.md#answer) a missing sleep state does not keep an image in development.

### Evidence

The declaration alone. No test gates a sleep state, and no hardware pass is required before a device declares one. Correcting a wrong declaration is ordinary maintenance.

### Day one

All six devices declare no sleep state: Odin 2 Portal, RP Mini V2, RG353M, RG DS, R36T Max, and RG35XXSP. Every one of them records the limit. No Korri image has ever suspended or resumed, and the Odin disables all four sleep targets today.

### When hibernation returns

Three facts deferred it. An SD card holds the image at roughly 20 to 90 MB/s, so a big-RAM device needs about a minute to write it and about a minute to read it back. The card is removable, and a pulled card makes resume impossible. Hibernation also needs swap, and the `r36tmax`, `rgds`, and `rpminiv2` module checks assert `swapDevices == [ ]` today.

Two decisions already hold for the day it returns. Which storage may hold the image is deferred with it. When a resume fails and the live session is gone, Korri starts normally and says nothing.

### Costs and limits

- Every device ships its first supported image with the no-sleep limit recorded, so the sleep-and-wake acceptance case fires on no device at first release.
- Light sleep ends a live session that the owner did not end. It is the one place where Korri destroys a session by a timer.
- Nothing catches a wrong declaration. A device that declares deep sleep without having it promises a freeze and thaw that fails in the owner's hands.
- The Odin keeps its four disabled sleep targets and its ignored lid switch until it declares a state. Its vendored ROCKNIX fake-suspend text stays unported.
- Naming hibernation before building it puts a state in the model that no acceptance case exercises.
- This answer authorizes no schema, option name, kernel work, firmware change, or deployment.

## Comments

### Source recheck, 2026-09-20

Read at `main` `77f7e2d4` in worktree `docs/device-sleep-tiers`. No device was contacted. No build was run.

#### What Korri declares today

| Device | SoC | Sleep declaration | Source |
|---|---|---|---|
| Odin 2 Portal | SM8550 | `systemd.targets.{sleep,suspend,hibernate,hybrid-sleep}.enable = false`, `HandlePowerKey = "ignore"`, `HandleLidSwitch = "ignore"`. The comment says real S3 does not work on this SoC under mainline, that ROCKNIX and legacy use "fake suspend" instead, and that the wake path is untested, so a failed resume looks like a hang. | `nix/devices/odin2portal/platform-policy.nix:150-166` |
| RP Mini V2 | SM8250 | `swayidle` powers `DSI-1` off after 300 s, on at resume, off `before-sleep`. Screen power only; no sleep state. | `nix/devices/rpminiv2/portal.nix:18-23,148` |
| RG353M | RK3566 | Nothing. | grep over `nix/devices/**/*.nix` |
| RG DS | RK3568 | Nothing. | same |
| R36T Max | RK3326 | Nothing. | same |
| RG35XXSP | H700 | Nothing. Clamshell hall sensor reports `SW_LID` on PE7. | same, `nix/devices/rg35xxsp/README.md:14` |

Odin is the only device in the tree that decides sleep policy at all. No shared module in `nix/base/` sets a sleep target, a `logind` handler, or an idle rule, so the other five inherit systemd defaults that no session has exercised.

inputd names three power actions that no device can reach. `PowerSuspend`, `LidClosed`, and `LidOpened` all carry `Trigger::Unsupported` (`services/inputd/src/action_catalog.rs:216-231`), and `is_reachable` returns `false` for that trigger (same file, line 342).

SoC mapping is taken from the [ROCKNIX emulator-default evidence](../evidence/rocknix-recommendations.md), which recorded each device's ROCKNIX platform page.

#### What ROCKNIX publishes

Read from the ROCKNIX distribution tree at branch `next`, commit `60ef1973b96e38f484255bbf0d33f34a3969823a`. One setting, `system.suspendmode`, selects between two mechanisms:

- `mem` writes systemd's `SuspendState`; the kernel then chooses `s2idle` or `deep` from what firmware offers. Real suspend.
- `off` writes `AllowSuspend=no` (`packages/rocknix/sources/scripts/suspendmode`), after which `rocknix-fake-suspend` handles the power key. That script exits immediately when hardware suspend is enabled, so the two mechanisms never overlap.

Fake suspend is not a sleep state. It turns the backlight or the compositor output off, mutes audio, disables LEDs, sets the CPU and GPU governors to powersave, parks every non-boot core, freezes the running game's processes, and then, by default, shuts the device down 900 s later through the EmulationStation shutdown API. It refuses to act while HDMI is connected. Its triggers are the power key and `SW_LID`, watched by the `input_sense` script, not by `logind`.

| ROCKNIX platform | Korri device | Published policy |
|---|---|---|
| H700 | RG35XXSP | `suspendmode mem`. The quirk deletes the older `off` plus `HandlePowerKey=ignore` override once, behind a `.h700_sleep_migrated` flag. |
| RK3566 | RG353M | `suspendmode mem`. |
| SM8550 | Odin 2 Portal | `suspendmode off`, `HandlePowerKey=ignore`, `HandleSuspendKey=ignore`, under the comment "Sleep is currently broken, so we'll disable it." |
| SM8250 | RP Mini V2 | No suspend quirk, so no published policy either way. |
| RK3326 | R36T Max | No suspend quirk. Its kernel tree carries two px30s suspend patches. |
| RK3568 | RG DS | ROCKNIX has no RK3568 platform. |

H700 real suspend is a firmware dependency, not only a kernel one. ROCKNIX builds `h700-suspend-stub`, an SRAM stub for PSCI `SYSTEM_SUSPEND` embedded into BL31 against U-Boot's own DRAM sources, and patches ATF with `001-allwinner-h616-psci-system-suspend.patch`. Three further H700 kernel patches gate the display pipeline across suspend, suspend the codec, and mark a 480 MHz suspend OPP. Korri's RG35XXSP ships mainline U-Boot 2025.10 with `armTrustedFirmwareAllwinnerH616` and no stub (`nix/devices/rg35xxsp/uboot.nix:2,14`).

#### Not known

No Korri image has suspended or resumed on any of the six devices. No session has tested a wake path. ROCKNIX's `mem` on H700 and RK3566 is evidence of an upstream route, not a Korri capability.
