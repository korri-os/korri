---
status: accepted
---

# `korri` has an author area

**Owner decision, 2026-10-28.** This changes ADR 0031, which made `korri` the
device owner's tool that mirrors the portal and talks to korrid on its own
device.

Plugin authors use the same `korri` command. Under `korri plugins` it gains the
author actions that a plugin template needs: start a new plugin, build it,
publish it to the author's own plugin repository, and sideload a build onto a
device with `--device`. Publishing signs the build (ADR 0044). Sideloading
still needs the owner's approval on the target device. Starting a new plugin
writes the plugin contract's TypeScript types into the project, so `tsc` works
with no Core checkout and no public package registry.

These actions have no portal equivalent, and building and publishing run with
no korrid. This is the one exception to ADR 0031's mirroring rule. The exact
verbs and arguments are decided when the commands are built.
