---
status: accepted
---

# Outside plugins live in their authors' repositories

**Owner decisions, 2026-10-28.** The goal is that a person who knows no Nix and
no Korri internals can write a plugin and share it. The plugin seam keeps every
affordance it has today.

A shared repository where outside authors open pull requests would make the
project the distributor of everything in it. Plugins that reach copyrighted
material could then take the whole project down. Tachiyomi is the precedent.
Its official extension repository listed extensions for sites that carried
Kakao's webtoons. Kakao contacted contributors on 2024-01-02, and the project
ended on 2024-01-13. Its successor, Mihon, ships no extensions and lets a
person add an outside repository by address.

## Decision

- Korri hosts no plugin made by an outside author. Each author hosts their own
  plugin repository.
- Korri hosts no index or list of outside plugin repositories.
- The portal ships with no outside repository address built in.
- `@korri` publishes only content that is licensed for redistribution. A
  `@korri` acquisition provider serves only sources that publish their content
  legally.
- Outside plugins get every affordance that first-party plugins get, including
  native services. Approval shows each privileged request, and the device owner
  decides.
- An author can put a build on their own device without publishing it. The
  build is still signed, and the device owner still approves it. No signature
  or permission check is turned off.

## Consequences

Nobody finds an outside plugin unless its author shares the address. Korri
does not judge outside repositories, so a device owner who adds one trusts it
on their own judgment. Approval and the sandbox limit what a plugin can do;
they do not stop an owner from trusting a bad author.

This is not legal advice. Ask a lawyer before publishing an official catalog
or a new `@korri` acquisition provider.

Open: how a device trusts an outside publisher, how a sideloaded build reaches
the device, and the contract stability promise to outside authors (#325).
