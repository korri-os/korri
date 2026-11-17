# No device keeps the person key

**Owner decision, 2026-11-16.**

At setup, the device that makes the person key shows its 12 recovery words
(ADR 0072). The person confirms the words, and then the device erases the
person key. After setup, no device holds it. The paper is the only copy.

The reason: a device that kept the person key would give a thief everything.
The thief could admit devices, remove the person's devices, and undo the
person's own undo. With no copy on any device, a stolen device can do only
what any owned device can do (ADRs 0069 and 0071), and the person key can
still overrule it.

## Considered options

- **The first device keeps the person key.** Undoing a removal would need no
  paper. Rejected: that one device would be worth everything to a thief.
- **The first device keeps the person key, encrypted with a password
  (NIP-49).** Rejected: anyone with the device can guess the password offline,
  with no limit on tries, and a password short enough to type with a
  controller is short enough to guess.
- **A passphrase on the recovery words (the BIP39 "25th word").** The paper
  alone would be useless to a finder. Rejected: one more secret to remember,
  and forgetting it makes the paper useless to the person too.

## Consequences

- To undo a removal, or to recover after every device is lost, the person
  enters the recovery words, or scans their QR code.
- The person must confirm the words at setup. Setup cannot end before that.
- A device never needs a local signer that holds the person key after setup.
