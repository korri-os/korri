# Decide device sleep tiers

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: claimed
Blocked by: 07

## Question

Device sleep arrives in tiers. Some devices have no deep sleep yet; later tiers may include ROCKNIX-style workarounds and hibernation ([Decide the shared product and hardware boundary](07-decide-product-device-boundary.md#sleep)). [Decide what happens when leaving and returning to a game](09-decide-live-session-behavior.md#leave-by-device-sleep) already fixes the behavior once sleep exists: freeze the exact launch before suspend, thaw on wake only if the user was in the game. What are the tiers, which tier does each of the six devices sit in today, and does a device with no sleep qualify as a supported image?

Inspect what each device declares now: [Odin disables sleep, suspend, hibernate, and hybrid-sleep](../../../nix/devices/odin2portal/platform-policy.nix), the RP Mini V2 OLED idle service in [portal.nix](../../../nix/devices/rpminiv2/portal.nix), and `PowerSuspend` in [inputd's action catalog](../../../services/inputd/src/action_catalog.rs), which has no trigger on any device. Check ROCKNIX's published behavior for the same boards before naming a tier after it; do not assume a workaround works on mainline kernels.

Resolve with Simon the tier names and what each promises to the user, whether tier "none" blocks supported status under [Define hardware limits and product acceptance](02-define-device-support.md#answer) or is a recorded limit, what the power button does on a device with no sleep, and what evidence proves a tier on hardware. Record decisions, not kernel work.
