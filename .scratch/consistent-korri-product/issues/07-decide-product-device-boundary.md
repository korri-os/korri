# Decide the shared product and hardware boundary

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: claimed
Blocked by: 02, 04, 08, 09, 10

## Question

Which integration decisions belong to one shared Korri product, which belong to hardware support, and what checks prevent a new device or product change from silently omitting required behavior?

Use the agreed product, hardware support policy, bundled-plugin lifecycle, user-identity lifecycle, live-session behavior, and ROCKNIX-based emulator selections. Image sizing and variant/first-use readiness planning were explicitly removed from this effort; do not reopen them as hidden prerequisites. Inspect current ownership rather than assuming a new abstraction is needed. Load `/codebase-design` in addition to `/grilling` and `/domain-modeling` for this ticket.

Start with [shared device policy](../../../nix/base/default.nix), [SD format composition](../../../nix/formats/sd-card.nix), [Linux host composition](../../../services/inputd/nix/korri-linux-host.nix), [portal integration](../../../clients/portal/nix/nixos-module.nix), and the device modules that import them. Review the actual [image](../../../.github/workflows/device-images.yml) and [cache](../../../.github/workflows/nix-cache.yml) workflows too; shared source alone does not provide shared delivery.

Real cases include repeated runtime-account and package wiring, RP Mini V2 disabling shared streaming services, and Odin selecting a different kiosk path. Recheck each before using it as evidence. Separate legitimate hardware constraints from missing integration and obsolete product paths.

Resolve ownership, allowed variation, delivery obligations, and the observable acceptance contract with Simon. Include the cost and limits of the selected design. Record decisions and source pointers, not a speculative configuration schema. Newly exposed migration or recovery choices become child decision tickets or specific remaining fog. Finish this map only when no decisions remain before `/to-spec`.
