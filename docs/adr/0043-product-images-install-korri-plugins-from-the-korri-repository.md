---
status: accepted
---

# Product images install `@korri` plugins from the `@korri` repository

**Owner decision, 2026-10-28.** Today each image pins every preinstalled plugin
by exact store path, once per architecture, in
`nix/product/published-plugins.nix`. Each publisher batch also needs
`paths-SYSTEM.txt`, `revision.txt` and offline metadata archives, plus a prose
record in `nix/product/PUBLISHED-PLUGINS.md`. Adding one plugin touches about
five files.

The owner decided to replace this now, before the outside-author lane exists,
instead of after it. An image will install its `@korri` plugins from the same
plugin repository format that every other device owner uses. This makes the
repository format load-bearing for offline recovery from the start.

Owner approval of exact bytes, the bound `@korri` key and offline recovery stay
in force. The format of what an image pins is not decided; it must be grounded
in the existing catalog (`services/korrid/plugin-host/src/catalog.rs`) or
chosen by the owner. An offline device cannot ask a cache for signatures, so
the image must carry each signature with its archive, as the offline metadata
archives do today.
