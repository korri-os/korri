---
status: accepted
---

# The stream viewer and every binary that links it are GPL-3

**Owner decision, 2026-11-11.** Korri accepts GPL version 3 for its stream
viewer and for every binary that links the viewer.

Korri's stream viewers build on `moonlight-common-c`, which is GPL version 3.
Issue #391 moves the viewer from separate upstream programs into Korri's own
code, so Korri code and GPL code will share a binary. The owner accepts the
licence there, because the other paths cost more than the licence does.

## Considered options

- **Keep the viewer a separate program for good.** This keeps Korri's other
  binaries apart from the GPL work. It rules out a viewer that Korri code
  links, which is what issue #391 asks for.
- **Use a protocol core that does not derive from `moonlight-common-c`.** The
  author could not read `moonlight-common-c` while writing it. This is the
  most work of the three paths.

## Consequences

- Korri distributes each binary that links the viewer under GPL-3, with its
  source.
- Slint has three licence options. In a binary that links the viewer, Korri
  uses Slint under GPL-3.0-only.
- A Rust port of the protocol core may derive from `moonlight-common-c`.
- This decision does not say which binaries link the viewer. It does not
  choose a licence for Korri code that does not link the viewer. On this
  date the repository root has no licence file.
- No lawyer has reviewed this reading of the licences.
