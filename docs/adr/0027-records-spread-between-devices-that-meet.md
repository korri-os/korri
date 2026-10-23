# Records spread between devices that meet

**Owner decision, 2026-10-22.** Design in progress. The disk-space rule and the
wire format are not decided.

No Korri device is ever designated to store data for the others, and any device
can be offline at any time. So a record, such as a save, reaches a device that
was off by spreading: any two devices of the federation that are online together
copy it to each other. It arrives if any device that holds it is online at the
same time as the device that needs it.

## Decision

- Devices that can run a game keep the records for that game.
- Other devices pass records on only while they have free disk space, and they
  remove the oldest copy first.
- Small records are also kept encrypted on public Nostr relays (ADR 0026). A
  large record never goes to third-party storage, such as a Blossom server.

## Considered options

- Every device keeps every record. This was rejected because a large PC save
  can fill a small handheld that cannot run the game.
- Encrypted storage on a Blossom server is always reachable. It was rejected
  because it is third-party storage, usually paid, with size limits.

## Consequences

- A record can be lost if every device that holds it stays offline until the
  device that made it is gone.
- ADR 0014 conflict handling by ancestry still decides between two saves;
  spreading changes only how a save arrives.
