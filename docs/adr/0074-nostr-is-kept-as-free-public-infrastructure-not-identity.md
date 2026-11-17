# Nostr is kept as free public infrastructure, not identity

**Owner decision, 2026-11-16.**

Korri keeps Nostr for four things only. Public relays are a free mailbox where
two devices on different networks find each other, with no server that Korri
runs (ADR 0026). Relays keep small encrypted records while a device is off
(ADR 0027). Relays hold the public vouch and removal record (ADR 0069). And
Nostr's keys, signatures, encryption (NIP-44) and recovery words (NIP-06) are
standard, with libraries korrid already uses.

Korri does not use Nostr as the person's social identity (ADR 0073), and a
join does not go through a phone signer over NIP-46.

## Considered options

- **The BitTorrent DHT (Mainline, as pkarr uses it).** Also free, with no
  server. Not chosen: it holds only tiny signed records and has no mailbox for
  a device that is off.

## Consequences

- Korri depends on public relays staying free. Relays can rate-limit, drop
  data, or refuse to store short-lived messages.
