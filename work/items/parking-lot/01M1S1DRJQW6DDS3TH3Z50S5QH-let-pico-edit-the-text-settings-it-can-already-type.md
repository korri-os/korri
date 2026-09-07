---
id: 01M1S1DRJQW6DDS3TH3Z50S5QH
slug: let-pico-edit-the-text-settings-it-can-already-type
title: Let Pico edit the text settings it can already type
origin: parked
status: To Do
priority: medium
labels:
  - surface
  - pico
  - settings
created: 2026-09-05
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: main
  commit: ecaf75af
  repo: korri
  invoked_by: se-work
---

# Let Pico edit the text settings it can already type

## Why it matters

Settings rows whose interaction kind is text or sensitiveText show NO KEYBOARD YET and refuse to edit. That was honest when written — Pico had no way to type — but the find screen now ships a working on-screen keyboard (PicoKeyboard, letters and digits, space/backspace/clear), so the refusal is stale. A device name is one of the few settings a person actually wants to change on the device itself, and today it can only be changed from another surface. The work is lifting PicoKeyboard into a shared editor overlay and calling changeSetting with the typed value; sensitiveText additionally needs masking and Korri's clearLabel honoured, which is a real design decision rather than a mechanical extension.

## Acceptance Criteria

- [ ] A settings row with interaction kind text opens an editor seeded with its current value and sends changeSetting with the typed result
- [ ] A sensitiveText row masks what is shown and offers Korri's clearLabel when one is published
- [ ] Cancelling leaves the value untouched and asks Korri nothing
- [ ] The NO KEYBOARD YET notice and its test are removed rather than left as dead reassurance
- [ ] surfaces/pico stays green

## Related

- `surfaces/pico/src/pages/PicoSettings.tsx`
- `surfaces/pico/src/ui/molecules/PicoKeyboard.tsx`
- `surfaces/pico/src/pico-settings-view.ts`
