# Define hardware limits and product acceptance

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: open
Blocked by: 01

## Question

When hardware cannot deliver an agreed product behavior, what does Korri promise, and when does an image qualify as a supported product rather than a bring-up candidate?

Use the intended-product decision and real cases. The charting review found a console-only RG35XXSP composition, a local-only RP Mini V2 composition, and different levels of input, audio, and streaming support. Recheck their current state. A missing driver, an absent physical capability, and an intentionally unselected plugin are different cases; do not classify them all as unsupported hardware.

Read [repository federation rules](../../../AGENTS.md), [RG35XXSP composition](../../../nix/devices/rg35xxsp/default.nix), [RP Mini V2 composition](../../../nix/devices/rpminiv2/portal.nix), and the hardware evidence for the behaviors under discussion. Existing federation rules allow another device to fulfill content requirements; they do not prove a working route exists.

Resolve with Simon what stays consistent, what can vary, how limitations must be communicated, and which failures block claiming product support. Define observable acceptance obligations, not a new capability schema or a visual design.
