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

## B2 implementation schema

The implementation is `services/korrid/src/federation/`. It has one aggregate
mutation owner and a private atomic-file helper. No registry, coordinator,
PeerList wire, or UI is wired in B2.

The exact JSON fields below implement the approved new document. Optional
values are explicit JSON `null`, not omitted fields. Missing fields and unknown
fields are errors. There is no fallback reader or migration.

| Location | Field | Grounding and reason |
| --- | --- | --- |
| Document | `localDevicePublicKey` | Existing device identity. Reject a document copied to another private identity. |
| Document | `ownerPublicKey` | Existing local owner-statement author, or `null` before binding. An established owner cannot change through a memory reload. |
| Document | `publication` | `null` before first publication, otherwise `{ generation, createdAt }`. `generation` is the existing EndpointRecord counter; `createdAt` is the outer NIP-01 event timestamp. Both must survive restart because relay replacement uses timestamp before event ID, not endpoint generation. |
| Document | `peers` | Approved object keyed by canonical device public key. Local identity is excluded. |
| Each peer | `ownerStatement` | Complete signed owner-event JSON string. Re-verify it on load and derive owner, device, binding event ID, timestamp, and status with the B1 verifier. Do not persist a second unverifiable copy of those fields. Only owned peers are visible. |
| Each peer | `endpointEvent` | Complete encrypted, signed endpoint-event JSON string, or `null` before discovery. Re-verify its signature, recipient/tags and content on load with the existing endpoint decoder. Its EndpointRecord contains the original device, owner, generation, ordered candidates, issue time, expiry and optional metadata. Retaining the original evidence permits authenticated restart recovery without duplicating record fields. |
| Each peer | `firstSeen`, `lastSeen` | Local observation times in Unix seconds. The brief and legacy peer memory ground the first/last-observation concepts; integer seconds match the existing Rust identity/endpoint clock. First observation is preserved. Last observation never decreases with clock rollback or stale binding/endpoint snapshots. |

No key material, pass secret, or new revocation container is added. Optional
`EndpointRecord.label` means existing `HostPayload.title`.
`EndpointRecord.moonlightAddress` means existing
`UpstreamHostConfig.moonlightAddress`. These authorized wire additions preserve
all original record fields and candidate order. Each metadata value is at most
256 UTF-8 bytes, nonblank, and contains no control characters. The values remain
device assertions, accepted only after the signed author joins current verified
same-owner membership. They do not override static configuration in B2; that
composition belongs to B3.

### Terminal revocations and one authorization authority

The directory receives the **same `Authorization` clone** as secure RPC, loaded
for the same private root. It does not create a second authorization cache.
Owner-authored revocation evidence is re-verified against the current owner and
nonlocal device before entering the existing `attempt`/`commit_revocations`
persistence API. The local signed owner statement identifies the authorized
owner context. This is ingestion of signed public owner evidence, not acceptance
of an RPC carrying unverified revocations or a replay nonce.

The existing `identity/authorization-revocations/<event-id>.json` files remain
the sole negative-evidence store. Their schema and terminal interpretation are
unchanged. Accept the whole verified revocation batch into shared authorization
memory before attempting its first file write. Track unsynced event IDs only in
memory. Every `commit_revocations` call retries pending writes, including calls
with duplicate or empty evidence. Remove an ID from pending state only after
both file and directory sync succeed. A storage failure still returns an error;
it never restores authorization for an accepted revocation.

Persist those signed events before removing peers from `peers.json`. This is
intentionally not a cross-file transaction: an aggregate write or size failure
can leave a revoked peer in the old memory document. Snapshots, endpoint/state
updates and later owned evidence consult the shared authority, so even tokens
whose epoch did not advance on failure cannot re-enable that peer. A later owned
event cannot override a recorded revocation, even if its timestamp is newer.
Shared authorization clones see directory revocations despite storage errors;
normal secure-RPC revocations also exclude directory peers through that authority.

Restart denial is guaranteed only after signed-event persistence succeeds. If
that write fails, the running authority denies immediately, but pending evidence
has no restart durability until a retry completes file and directory sync. Retry
directory ingestion with the signed revocation sources and a fresh work token;
a failed call is not durable success. Once synced, authorization excludes any
old aggregate entry and removes it on the next directory load.

### Endpoint and work ordering

An endpoint update accepts the original signed encrypted event, never a bare
`EndpointRecord` claiming an owner. The directory checks author/device equality,
current membership and owner equality at commit. Its high-water order is exactly
the existing relay order: higher generation, then higher `issuedAt`. The first
accepted record wins an equal pair. Equal/stale records cannot overwrite
candidates, labels, or observation time, including after restart.

Expiry removes only current-relay eligibility. The same authenticated endpoint
remains an indefinite remembered fallback and retains its generation high-water
mark. Load verifies historical endpoint evidence at its original issue time;
only the current projection applies today's expiry. New expired announcements
are not accepted. B3 still owns strict HTTP(S) origin validation and candidate
failover; B2 preserves the existing endpoint URL semantics.

`begin_work` returns an identity/configuration/membership token. Changes to those
inputs invalidate stale commits. `begin_peer_work` also assigns a per-peer
attempt sequence before network I/O. A newer attempt defeats an older completion
without invalidating parallel work for other peers. Live states are only
`Loading`, `Ready`, and `Failed { error }`, with a local update time; they are not
persisted or generated wire types. Restart returns visible peers to Loading.

`reserve_publication` durably increments generation and advances NIP timestamp
to `max(now, previous + 1)` before returning either value. Both increments are
checked. A timestamp more than 300 seconds ahead of the wall clock fails rather
than publishing an event outside the existing future-skew bound. B4 must use the
returned timestamp for both `EndpointRecord.issuedAt` and the outer publication
`created_at`, and derive expiry from it. Failed publication burns a reservation.
Hosts with no advertised candidates must not reserve or publish an empty record.

### Private I/O and limits

The private root and federation directory must be `0700`; the document must be
`0600`. Validate path ancestors and reject symlinked directories, linked files,
nonregular files, incorrect modes and oversized reads. One process may open only
one directory writer for a root; consumers clone it. The store checks its last
accepted bytes before updates, so missing/replaced/rolled-back files cannot
silently lower an active process's high-water marks. Writes use a create-new
random temporary, file sync, atomic rename, and parent-directory sync. These
patterns are grounded in `identity.rs`, not claimed to resist hostile concurrent
pathname replacement by the private directory owner.

Only a previously absent federation directory initializes empty memory. An
existing directory with no document is an error, including an interrupted first
initialization. Corrupt or missing publication fields never reset the counter.
There is no rollback-proof external anchor: deletion of the entire federation
directory, or restoration of a complete older private-state backup while the
process is stopped, cannot be distinguished from first use or older valid state.
Do not describe the private file as protection against that threat.

Bounds are 8 MiB per memory document, 1,024 remembered peers, and 256 signed owner
sources per update (B1 returns at most 128 devices with latest and revocation
evidence). Existing identity/relay limits still bound each source and candidate.
A full document returns an error; it does not evict peers or lower ordering state.
Live error text is limited to 512 bytes without control characters. Callers must
supply sanitized errors, not remote response bodies or secrets.

Cost: memory reads verify retained signed evidence and use private filesystem
checks. Network I/O stays outside both aggregate and shared-identity locks, but
private I/O runs inside the serialized mutation. One live process owns each
private root; the open guard is not a cross-process lock. Remembered peers remain
usable during relay outage, so unseen revocations remain delayed indefinitely.
