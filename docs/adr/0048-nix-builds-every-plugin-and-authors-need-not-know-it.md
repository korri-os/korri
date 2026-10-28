---
status: accepted
---

# Nix builds every plugin, and authors need not know it

**Owner decision, 2026-10-28.** A plugin author who has never used Nix must be
able to get a plugin onto a device. If Nix is a first step, fewer people will
write plugins. Nix still gives Korri exact builds, signed caches and the whole
of nixpkgs, and the owner wants to keep all of that, including full Nix access
for authors who know it.

## Decision

- Nix stays the only tool that builds and packages a plugin. Korri does not
  reimplement Nix's archive, store path or signing rules in its own code.
- Korri's author tooling runs Nix for the author. A new author can get a
  plugin onto their own device without installing, configuring or learning
  Nix.
- Getting started needs no CI and no account on a hosting service. A CI
  template is optional, for authors who want it.
- An author who knows Nix keeps full control: their own `plugin.nix`, their
  own flake, and every builder input.

## Considered options

`korri` could package source-only plugins by writing the Nix archive and
computing its store path itself, on any operating system. It was rejected
because it is a second implementation of Nix formats that must match Nix
exactly, forever.

Bundling Nix inside `korri` works on Linux, for example with nix-portable.
macOS would also need a Linux builder, and Windows has Nix only through WSL.
