# Ship the streaming host as a removable plugin and delete the shared-host composition

Status: ready-for-agent
Blocked by: 02, 07, Phase 2 gate

## What to build

Ship stream hosting as a normal approved plugin that owners can disable or remove. Delete the old shared-host Sunshine composition in the same cut so no hidden unit, socket, group, rule, or package remains.

## Acceptance criteria

- [ ] The implementation follows the human decision from ticket 07 for certificate provisioning and input-seat ownership. It adds no Sunshine-specific knowledge to korrid beyond that approved boundary.
- [ ] The plugin uses its real native service artifacts and requests every unit, directive, capability, device, and other authority through the approval path from ticket 02.
- [ ] The old Sunshine service, certificate socket, seat service, group, udev rules, package assertions, and per-device shared-host composition are deleted in the same cut.
- [ ] The streaming host is in the default plugin selection for every current device model except R36T Max. R36T Max has a recorded missing-encoder limit and no disabled unit.
- [ ] One plugin release per Nix system carries the encoders supported by that architecture. Sunshine selects at startup; korrid performs no encoder match and reads no encoder fact.
- [ ] The plugin declares its stream TCP and UDP ports. The plugin host opens exactly those ports on every interface and leaves the administrative port closed.
- [ ] Disable, failed enable, rollback, and removal withdraw the firewall rules. Removal also stops the daemon and removes every plugin-owned unit, socket, group, and device rule.
- [ ] Removal reclaims eligible storage before success and preserves owner data under ticket 03's lifecycle.
- [ ] With the plugin removed, the device boots to the portal, accepts input, and completes an authenticated RPC call to its local korrid.
- [ ] Approval tests prove that the report names all widened authority and that every unnamed request is refused by name.
- [ ] A recorded enabled-device check demonstrates pairing and streaming, and each tested device records the encoder selected at startup with CPU load and temperature. These records do not create numeric performance targets.
- [ ] No compatibility unit, disabled legacy service, fallback package, or dual composition remains.
