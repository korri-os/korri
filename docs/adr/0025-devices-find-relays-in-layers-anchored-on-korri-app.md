# Devices find relays in layers, anchored on korri.app

**Owner decision, 2026-10-22.**

**Partly superseded by ADR 0026 (2026-10-22):** devices use public relays
only; layer 3 is dropped.

A Korri device must still reach its other devices after being switched off for
years. Public Nostr relays do not last that long: relay.nostr.band, one of the
two relays every image shipped, was dead on 2026-10-22, and Nostr has no
built-in way to find live relays once a device's whole list is dead. NIP-65 and
NIP-66 help only after the device reaches a live relay.

So devices look for each other in this order:

1. The local network, with no server.
2. A relay list published under `korri.app`, a domain the owner holds. If every
   relay an old image knows has died, one change under that domain fixes every
   device.
3. A Korri relay on a free Cloudflare Workers and Durable Objects plan, listed
   first.
4. A few long-lived public relays in the image, and relays learned from the
   person's other devices.

The relay list follows NIP-05: `https://korri.app/.well-known/nostr.json` with
its `relays` field, served by Cloudflare Pages. The list is not signed. Every
message on the relays is signed, so a wrong list, for
example after someone takes over a lapsed domain, causes an outage but cannot
forge a join or read data.

The cost is a domain renewal each year and a free Cloudflare account. If the
domain lapses, devices fall back to the relays in their image. If Cloudflare
ends the free plan, the relay costs about $5 a month or moves.

## Considered options

- **The image's relay list only, updated by system updates.** Rejected: a device
  that misses updates for a long time can lose every relay.
- **The korri.app list plus public relays, without a Korri relay.** Rejected:
  public relays filter non-standard events and rate-limit clients that are not
  their own apps.
