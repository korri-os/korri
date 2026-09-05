# Bundle B peer-memory decision

The owner selected `new-private-document` in decision
`f87fb4a1-e605-4726-b2cb-11487e6b2d13` on 2026-09-05.

This approves the implementation plan's new private federation document with
peers keyed by device public key, plus the minimum security state needed to
preserve ordering across restarts. The plan places it at
`private_state_root/federation/peers.json`. This is an approved change from the
legacy JSON array, not a claim that the new layout was harvested from legacy.

The implementation must cite the existing contracts for each stored fact.
Reuse the signed owner-statement verifier, existing signed revocation storage,
and `EndpointRecord` identity, generation, ordered candidates, issue time, and
expiry semantics. Keep verified revocation evidence when removing a visible
peer, so an old relay record cannot restore it after a restart.

Store no private keys or pass secrets in peer memory. Keep private directories
at `0700` and files at `0600`. Preserve known peers indefinitely unless their
owner binding is revoked. Relay failure or endpoint expiry does not erase
remembered candidates or security ordering evidence.

Cost: the new document departs from legacy and adds persistence tests. Offline
revocation remains delayed until relay access returns. This decision does not
approve cross-owner discovery, a new capability model, UI work, or a new
revocation protocol.

Grounding: `legacy:product/apps/portal/peers/peer-store.ts`,
`docs/research/federation-restoration-brief.md`,
`services/korrid/src/identity.rs`, `services/korrid/src/authorization.rs`, and
`services/korrid/src/relay.rs`.
