# No device keeps the person key

**Owner decision, 2026-11-16.**

At setup, the device that makes the person key shows its 12 recovery words
and their QR code (ADR 0072). The person confirms the words, and then the
device erases the person key. After setup, no device holds it. The paper is
the only copy.

The reason: a thief who held the person key could undo the person's
undo, so the person could not take back their devices. With no copy on any
device, a stolen device can do only what any owned device can do (ADRs 0069,
0071 and 0075), and the person key can still overrule it.

## Considered options

- **The first device keeps the person key.** Undoing a removal would need no
  paper. Rejected: that one device would be worth too much to a thief.
- **The first device keeps the person key, encrypted with a password
  (NIP-49).** Rejected: anyone with the device can guess the password offline,
  with no limit on tries. A password short enough to type with a controller is
  short enough to guess.
- **A passphrase on the recovery words (the BIP39 passphrase).** The paper
  alone would be useless to a finder. Rejected: one more secret to remember,
  and forgetting it makes the paper useless to the person too.

## Consequences

- To undo a removal or an admission, or to recover after every device is
  lost, the person enters the recovery words or scans their QR code.
- The person must confirm the words at setup. Setup cannot end before that.
- After setup, a device holds the person key only while the person uses the
  recovery words, and then erases it. korrid still never holds it.
- Today the local signer keeps the automatic person key and can export it with
  a password (`local_signer.rs`), and an identity switch signs with the
  current person key (`identity_switch/coordinator.rs`). That code must
  change. Whether a switch away from an automatic identity (ADR 0023) needs
  its recovery words is not decided.
