# Recovery is twelve words and a QR code on paper

**Owner decision, 2026-11-16.** The QR code holds the 12 words as plain text,
so any phone camera shows them and the Korri app reads them (ADR 0073).

At setup, Korri shows the person key as 12 recovery words (BIP39, as Nostr's
NIP-06 uses them) and a QR code of them, for the person to keep on paper. The
person needs them only when every owned device is lost. While any one device
survives, it vouches for new devices and removes lost ones (ADRs 0069 and
0071), so no recovery is needed.

## Considered options

- **A password, with the encrypted key on public relays (NIP-49).** Nothing on
  paper. Rejected: the encrypted key is public, so a weak password can be
  guessed offline.
- **No recovery.** Rejected: losing every device would also lose the saves and
  the device list.

## Consequences

- 12 words come first and the key is made from them. A person key that was
  made at random, like the owner's current key, cannot become 12 words.
- NIP-06 is marked "unrecommended" by the Nostr spec, but many Nostr apps
  still read it.
