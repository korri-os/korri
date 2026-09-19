# Define hardware limits and product acceptance

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: resolved
Blocked by: 01

## Question

When hardware cannot deliver an agreed product behavior, what does Korri promise, and when does an image qualify as a supported product rather than a bring-up candidate?

Use the intended-product decision and real cases. The charting review found a console-only RG35XXSP composition, a local-only RP Mini V2 composition, and different levels of input, audio, and streaming support. Recheck their current state. A missing driver, an absent physical capability, and an intentionally unselected plugin are different cases; do not classify them all as unsupported hardware.

Read [repository federation rules](../../../AGENTS.md), [RG35XXSP composition](../../../nix/devices/rg35xxsp/default.nix), [RP Mini V2 composition](../../../nix/devices/rpminiv2/portal.nix), and the hardware evidence for the behaviors under discussion. Existing federation rules allow another device to fulfill content requirements; they do not prove a working route exists.

Resolve with Simon what stays consistent, what can vary, how limitations must be communicated, and which failures block claiming product support. Define observable acceptance obligations, not a new capability schema or a visual design.

## Comments

[Hardware limits and acceptance evidence](../evidence/hardware-acceptance.md) records the source and historical-test review. No current device behavior was verified.

### Supported versus development images

Simon selected development-until-complete. An image with missing applicable required product behavior remains a development image; documenting the gap does not make it a supported product image. Development and recovery images remain useful, but do not count as complete Korri installations. Genuine hardware limits are a separate decision, not an explanation to apply to unfinished drivers or integration by default.

### Required hardware features

Simon selected product-required hardware, not every built-in hardware feature. Hardware features needed for the agreed product must work. Other hardware features can remain unsupported if the limitation is explicit. An optional feature cannot be used as an excuse for missing required behavior; if a required content route depends on it, that route needs working support or a genuinely available alternative consistent with the existing federation rules.

### Release verification

Simon selected change-based retesting. The first supported image for each device model needs full physical acceptance using normal Korri setup without manual fixes. Later supported image releases need a boot-and-play check plus physical tests of affected behavior on the affected device models. Relevant earlier evidence can carry forward for unchanged behavior; identifying the impact of each change is part of the release work.

This reduces repeated device testing at the cost of possible gaps if change impact is assessed incorrectly. A successful build or configuration assertion alone is not physical acceptance. This decision grants no permission for a device write or owner-assisted test; existing approval and readiness rules still apply.

## Answer

Simon explicitly approved the complete support rules and closing this ticket. The agreements under [Supported versus development images](#supported-versus-development-images), [Required hardware features](#required-hardware-features), and [Release verification](#release-verification) are the binding policy.

Hardware variation can change available content routes. It cannot silently remove shared required Korri behavior. Missing drivers, unfinished integration, and unverified operation are not proof of a physical limit. A documented limitation in an extra hardware feature is allowed; a documented missing product requirement still leaves the image in development.

Support claims must match the evidence for the actual image and affected device model. The first full acceptance uses normal Korri setup without manual fixes. Later change-based verification retains relevant evidence for unchanged behavior while testing boot, play, and affected behavior on hardware. Detailed cases and any measured performance targets remain part of [Decide the shared product and hardware boundary](07-decide-product-device-boundary.md); no numeric targets or speculative capability schema were selected here.

The cost is fewer supported devices until their applicable requirements work, plus responsibility for correctly identifying regression risk on later releases. No current image or device was certified by this planning decision. Publication rights, device-write approval, and owner readiness for physical tests remain separate.

[The glossary](../../../CONTEXT.md) records the agreed meanings of supported image and development image.
