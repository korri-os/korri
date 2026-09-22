# Export a key backup and switch identity

Status: in-progress
Blocked by: 05

## What to build

Let an owner export an encrypted backup and deliberately replace the device owner with an imported or remote person key. A failed or interrupted switch must not expose half-moved data or take authority from the old owner.

## Acceptance criteria

- [ ] Settings can request a NIP-49 encrypted export from the signer and show it as text and a QR code.
- [ ] The export flow states plainly that Korri cannot recover a lost key and that a key not exported before device loss is unrecoverable.
- [ ] A replacement identity can come from a NIP-49 backup imported into the local signer or from the existing NIP-46 remote-signer path.
- [ ] Before commit, the new key signs the new owner binding and the owner confirms that every peer pairing and stream-client trust will be lost.
- [ ] The switch stages a new device key, obtains the new owner binding and old-owner revocation, then performs one atomic identity-directory swap.
- [ ] Any failure before the swap leaves the old identity, owner authority, and local records untouched.
- [ ] The old-owner revocation is published only if the old owner statement was previously published.
- [ ] When transfer is accepted, unsigned per-person records are re-keyed and the old key's local records cease to exist. Signed events are not re-signed.
- [ ] When transfer is declined, the old key's local records are deleted. There is no orphan state, fallback read, or dual ownership.
- [ ] If power fails after commit, journal recovery finishes before korrid serves per-person data.
- [ ] The old local key remains retired and exportable until a separate explicit delete action. `korrid identity reset` remains device-identity reset only.
- [ ] Tests cover export, import, remote signer, prepare failure, atomic commit, both data choices, interrupted recovery, conditional revocation publication, and retired-key deletion.
- [ ] Save files and save states remain under the existing fixed plugin account root; this ticket does not invent transfer behavior for them.
