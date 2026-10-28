---
status: accepted
---

# A device owner trusts outside publishers one device at a time

**Owner decisions, 2026-10-28.** This follows ADR 0042. Today a device trusts a
publisher only through a key and cache URL in its NixOS configuration
(`services.korri.pluginHost.publishers`), so a stranger's plugin cannot reach a
stock device. Nix itself accepts an unsigned content-addressed path; refusing
one is Korri's own rule (`services/korrid/plugin-host/README.md`).

## Decision

- Owner approval of exact files and the sandbox stay mandatory for every
  plugin, signed or not.
- `@korri` stays checked against a key written into the image. Nothing learned
  at runtime can stand in for it.
- **Signed lane, the default.** The author tooling creates the publisher key
  and signs every build, so the author never handles a key. A device pins the
  key the first time its owner trusts that publisher. The device learns the
  key from that first signed release, and the portal shows its fingerprint.
  Later releases must match it.
- **Unsigned lane.** A device owner can install an unsigned plugin after a
  disclosure that names the risk: Korri cannot confirm who made it, or that an
  update comes from the same person. The disclosure appears again for every
  update. Nothing can revoke an unsigned plugin except removing it.
- An unsigned plugin may use any namespace except one the device already trusts.
  If unsigned plugins already use a namespace, the device refuses to trust a
  signed publisher for it until the owner removes them.
- Unsigned plugins get the same affordances as signed ones, including native
  services. The disclosure and the privileged warnings both appear.
- **Key change.** A release signed by a key other than the pinned one is
  refused with "publisher key changed". The owner can trust the new key
  knowingly, as with SSH host keys. There is no rotation record.
- **Revocation.** The owner can revoke a trusted publisher from the portal and
  from `korri`. This has the same effect as removing a configured binding
  today: the publisher's packages no longer start, and stop and cleanup still
  work.
- Trust is per device. Trusting a publisher on one device does not trust it on
  the owner's other devices.
- A device owner can enter a repository address three ways: typed on the
  device, from a computer with `korri ... --device`, or passed from a phone.
  A QR code or phone link carries the plain `https` address of the plugin
  repository.

## Consequences

The first trust of a signed publisher trusts whoever answers at that moment.
An author who loses their key cannot ship updates under the same publisher.
Two unsigned plugins on one device can claim the same namespace. An owner with
several devices repeats each trust decision on each of them.

Handhelds have no camera, so "from a phone" means the phone reads the address
and sends it to the device. The remote `korri` path already covers repository
add through `--device`; trusting a publisher remotely is a further widening
that `clients/cli/README.md` does not allow today.

The storage and wire format of a pinned key, and how the disclosure is worded,
are not decided. ADR 0049 records how requirements across publishers work.
