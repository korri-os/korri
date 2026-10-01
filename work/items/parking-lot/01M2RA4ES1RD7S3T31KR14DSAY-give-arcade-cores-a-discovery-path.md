---
id: 01M2RA4ES1RD7S3T31KR14DSAY
slug: give-arcade-cores-a-discovery-path
title: Give arcade cores a discovery path
origin: parked
status: To Do
priority: medium
labels:
  - plugins
  - libretro
  - discovery
  - design
created: 2026-09-17
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: main
  commit: 13abd4f0
  invoked_by: se-work
---

# Give arcade cores a discovery path

## Why it matters

The MAME family, fbalpha2012 and FBNeo's romsets load zip archives and are identified by romset name, not file extension. korrid's discovery matches extensions only, so arcade content cannot reach a runner. FBNeo is in the catalogue but only its disc descriptors (cue/ccd) match, so its actual romsets are invisible. This is the one class of libretro core the current catalogue cannot express.

## Acceptance Criteria

- [ ] A zip arcade archive resolves to an arcade runner by romset or by filename, or the decision to leave arcade out is recorded
- [ ] The MAME family and fbalpha2012 either gain a catalogue entry or a written reason they do not
- [ ] Extension-only claims for arcade do not silently match unrelated zip archives

## Related

- `plugins/libretro/cores.nix`
- `services/korrid/src/plugin.rs`
- `services/korrid/src/discovery/scanner.rs`
