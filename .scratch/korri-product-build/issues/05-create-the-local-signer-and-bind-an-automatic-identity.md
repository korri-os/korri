# Create the local signer and bind an automatic identity

Status: resolved
Blocked by: None

## What to build

Give a new owner a real person key on first boot without a sign-in screen. Keep the private key in a separate local signer and bind it as the device owner without publishing anything.

## Acceptance criteria

- [ ] A separate local-signer service runs under its own uid and private directory and holds the person private key.
- [ ] korrid signs through the existing `PersonSigner` contract and never stores or reads the person private key.
- [ ] On first boot, the signer creates one automatic identity and korrid binds it as device owner through the existing owner-binding path.
- [ ] First boot shows no sign-in or identity-choice screen.
- [ ] Creating and binding the automatic identity publishes nothing to a relay and joins no federation.
- [ ] Restarting the signer or korrid preserves the same automatic identity and owner binding.
- [ ] The local signer cannot read korrid private state, and the product runtime account cannot read signer private state.
- [ ] Focused signer and identity tests cover key custody, first-boot creation, owner binding, restart, and failure before binding.
- [ ] The product VM test proves that first boot creates and binds the automatic identity without publication.
- [ ] Backup export and identity switching are not implemented here; ticket 14 owns them.
- [ ] The transport is grounded in the existing signer contract. No new identity or storage schema is invented for convenience.
