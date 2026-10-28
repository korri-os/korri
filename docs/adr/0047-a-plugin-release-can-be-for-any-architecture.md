---
status: accepted
---

# A plugin release can be for any architecture

**Owner decision, 2026-10-28.** Multiple architectures are first class, and a
device never downloads a release labelled for another architecture.

A plugin that ships native files has one release per architecture, as today.
A plugin that ships no native files is the same file on every architecture.
A test on 2026-10-28 built a source-only plugin for `x86_64-linux` and
`aarch64-linux`. Both gave the content-addressed path
`/nix/store/ajakzyzwgasca09kjma1z11yz1z4a1wg-korri-plugin`, with the same NAR
hash and no references. Such a plugin publishes one release with an explicit
"any architecture" label, as Debian does with `Architecture: all`.

This adds a value to the catalog's `platform` field
(`services/korrid/plugin-host/src/catalog.rs`). Its spelling is not decided.
A build that names any native file, service or exact required build cannot
carry the label, because those store paths differ per architecture.
