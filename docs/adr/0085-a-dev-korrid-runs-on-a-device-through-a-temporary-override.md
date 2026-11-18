---
status: accepted
---

# A development korrid runs on a device through a temporary override

**Context, 2026-11-17.** The only way to put a korrid change on a device is to
build a signed Nix bundle on fuji, copy it, and switch to it with
`korri-bundle-select switch`. The switch takes 5 to 6 s. The build takes
minutes, because crane caches only korrid's dependencies, so each change
compiles the whole korrid crate again. The owner wants a change on a device
quickly during spikes and playtests.

## Decision

- An agent may build korrid incrementally with cargo on fuji, which keeps a
  target dir between builds, and copy the binary to a device.
- The device runs that binary through a systemd override that lives in
  `/run`. A reboot removes it, and the signed bundle runs again.
- Code is never compiled on the device. Plugin signature and permission
  checks stay in force.
- Anything that must match a release, such as an acceptance record or a
  publication, still uses the signed bundle.

## Consequences

A development binary is not a Nix store path and carries no cache signature.
What runs on a device during a spike can differ from what ships. A
development binary must never be mistaken for a release build.
