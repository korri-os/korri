# Decide automatic Nostr identity and later replacement

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: open
Blocked by: 01

## Question

How can first use create a real Nostr user identity without mandatory sign-in, while allowing safe backup and an explicit switch to another identity later?

The accepted product direction is recorded under [User identity](01-define-intended-product.md#user-identity). Resolve the remaining lifecycle and security decisions here, not the product direction again.

Read the accepted [identity protocol](../../../docs/research/korrid-identity-protocol.md), the actual [device identity implementation](../../../services/korrid/src/identity.rs), the [person signer contract](../../../services/korrid/src/remote_signer.rs), and the [restricted test-owner replacement](../../../services/korrid/src/identity/offline.rs). Current person and device identities already use Nostr-format keys. The accepted protocol keeps the person's private key outside Korri and assigns generation and backup to the signer. Normal owner imports refuse changes to a different owner. The offline exception only retires the known test owner; it is not a general transfer feature.

Resolve with Simon who generates and holds the automatically created user key, what backup and recovery mean, and what consent is needed when switching identities. Any change to existing key custody or ownership security needs explicit approval. Keep device identity, active user identity, and device ownership distinct. Changing the active user must not silently transfer device authority.

The desired switch offers a choice to transfer local library choices, saves, and preferences without reinstalling. Define how that choice preserves or separates existing data and what happens to old access. Do not promise that another identity inherits old signatures or that other devices automatically trust it. Decide the safe behavior for failed or interrupted replacement before claiming a complete design.

Ground any storage or protocol proposal in the existing producer/consumer contracts or an explicit user choice. This ticket records decisions, not a new signer implementation or a device operation. Cross-device save synchronization remains deferred by the parent product decision; an explicit local data transfer does not authorize building synchronization.
