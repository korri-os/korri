# Widen plugin-host unit admission, bound by approval

Status: resolved
Blocked by: None

## What to build

Let a plugin request the native systemd units and authority that its real service needs, while making the administrator approve that exact reach. Keep every other plugin confined to the authority in its own approval.

## Acceptance criteria

- [ ] The plugin host can admit the multiple native units required by the existing streaming-host artifacts instead of limiting a plugin to one service.
- [ ] The admission policy accepts only the units, directives, devices, and capabilities requested by that plugin and named in its approval.
- [ ] The approval report names all widened authority before approval, including privileged directives, capabilities, and device access.
- [ ] Any unnamed unit, directive, device, or capability is refused by name.
- [ ] Approval for one plugin grants no authority to another plugin.
- [ ] The policy version contributes to the effective unit and approval digest through the existing approval path.
- [ ] Native configuration remains in native service files. `plugin.ts` stays effect-free and does not duplicate systemd configuration.
- [ ] Plugin-host unit-policy, native-unit, host-boundary, approval-report, and digest tests cover the allowed and refused cases.
- [ ] No streaming-host packaging cut is included in this ticket.
