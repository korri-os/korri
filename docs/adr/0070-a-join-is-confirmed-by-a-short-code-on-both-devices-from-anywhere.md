# A join is confirmed by a short code on both devices, from anywhere

**Owner decision, 2026-11-16.** Design in progress: the code length, the key
exchange and the screenless flow are not decided.

Both devices show the same short code, and the person taps Accept on each, as
in Bluetooth pairing. A device with no screen shows no code; the person types
a short code from the other device instead. The two devices can be on any
networks: they find each other through public Nostr relays (ADR 0026), not
only on the same home network or tailnet.

The new device finds the owned device the same way near or far: the person
types a short code from one device into the other (owner decision, 2026-11-16,
recorded with this ADR). Whether the compare-and-Accept step stays when a
typed code already ties the two devices together is open.

The code is what makes relays safe for this. A relay, or anyone who reads the
relay, cannot sit between the two devices without the codes differing.

## Considered options

- **QR code.** Rejected: many Korri devices have no camera.
- **Accept with no code.** Rejected: another device could race the real one.
- **Same home network or tailnet only.** Being close would be part of the
  proof. Rejected by the owner: he wants to join a device from anywhere.

## Consequences

- The 8-digit join code that the device-join design defined for discovery
  fallback is replaced.
- A join can fail when relays are down or blocked. Korri says so clearly.
- Comparing the code is the only check, so the person must really compare it.
