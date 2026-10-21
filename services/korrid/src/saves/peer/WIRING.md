# Save peer integration

The implementation is in new files only. It does not enable itself in product
startup. The integration patch is `/tmp/save-sync-slice-1/peer-wiring.patch`,
based on `e48cca979d17f9b294edf3dcadadb7e61b195165`. Apply it after these files.
It passed `git apply --check` against that base.

## Mechanical hooks (the patch is the exact code)

| File | Hook |
|---|---|
| `src/saves/mod.rs` | `pub mod peer;` |
| `src/lib.rs` | `RpcRequest::{SaveHeads, SaveRecords, SaveInUse, SaveObject}` with the approved request payloads; the first three `RpcResponse` variants use the existing tagged Ok/Err outcome grammar |
| `src/lib.rs` | `AppState.saves: Option<Arc<saves::peer::SavePeerService>>`; dispatch heads/records/inUse to that shared service; reject SaveObject in normal RPC dispatch |
| `src/authorization.rs` | All four peer variants use `PeerPolicy::OwnerDeviceOnly`. Service methods additionally require `AuthorizationContext::Peer(Principal::OwnerDevice { .. })`, rejecting browser and reconciliation contexts too |
| `src/portal_access.rs` | Add the four peer variants to the exhaustive non-initial-read arm. This is needed to compile, not permission to expose peer saves locally |
| `src/peer_rpc.rs` | Extract the existing signature, recipient, clock, authorization, revocation and nonce checks into `authenticate`. Both routes use that function and the same replay guard. Reject `SaveObject` on `/peer-rpc`, and every other method on `/peer-saves-object` |
| `src/peer_rpc.rs` | Load `saves/peer/object_route.rs` as a private child module and register `POST /peer-saves-object` under the existing 8 MiB request-body limit. The response is streamed bytes, not JSON |
| `typeshare.toml` | Map the existing untagged `SaveInUse` to `SaveInUseActive \| SaveInUseIdle`. Outcomes need Typeshare annotations because exported `RpcResponse` references them; regenerate contracts through the usual Core check |

The response is signed at completion time, not request time. A records push can
wait for a large object transfer; its authenticated response must not already
be outside the existing clock window when it is sent.

## Product composition and lifecycle hooks

Use the **existing** shared repository, credentials and federation directory.
Do not open another federation directory for this service. Given
`save_repository: Arc<dyn SaveRepository>`, `federation: FederationResources`,
`private_state_root: &Path`, and `save_in_use: saves::peer::InUse`:

```rust
let save_peer_service = Arc::new(saves::peer::SavePeerService::new(
    save_repository.clone(),
    federation.credentials.clone(),
    federation.directory.clone(),
    private_state_root.to_owned(),
    save_in_use,
)?);
lan.saves = Some(save_peer_service.clone());
local.saves = Some(save_peer_service.clone());
```

`InUse` has this exact signature:

```rust
Arc<dyn Fn(&str, &str) -> Result<Option<u64>, SaveError> + Send + Sync>
```

Its arguments are the authenticated person and game. The host must return the
pending/live session's `since` timestamp, including frozen and stopping
sessions. Return `None` only when that person/game is idle. No expiry applies.
Do not install an always-idle callback. The host's existing public status lacks
`since` and pending-owner details; the session coordinator must supply this
callback from its authoritative state.

After the session snapshot and every object are durable, invoke this on the
host's background-work owner (do not delay the portal behind network I/O):

```rust
let results = save_peer_service.after_snapshot(&record.game_id).await?;
for (_peer, result) in results {
    if let Err(error) = result {
        eprintln!("save push: {}", error.failure().code);
    }
}
```

On a verified same-owner endpoint/contact, including the transition to Ready,
invoke:

```rust
save_peer_service.on_contact(&peer.device_public_key).await?;
```

Integration points are the successful endpoint application in
`federation/coordinator.rs` and a successful reachability observation in
`upstream_native.rs` (`FederationDirectory::set_peer_state`). Keep lifecycle
ownership with the caller. The service checks the current verified roster
before each request and after each asynchronous response. It never trusts a
caller-supplied URL. Retrying on later contact is safe.

For the pre-launch check:

```rust
let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
let checks = save_peer_service.pull_before_launch(game_id, deadline).await?;
```

The caller owns Skip by dropping that future. Each `PeerCheckResult` contains
`device`, `in_use`, and `transfer`. Preserve a known `in_use` warning even when
`transfer` failed or reached the deadline. A failed check is not idle or Synced.
Use the repository's imported ancestry to decide split/choice. Only the session
boundary code may materialize a selected snapshot into the game's folders.

## Boundaries and costs

- Incoming records are validated by the native canonical record implementation.
  Objects must be named by that person's stored records. Hash and length checks
  precede publication. A declaration alone never receives confirmation.
- A push sends records; its receiver pulls missing objects over the signed
  object route, then returns confirmed IDs. The receiver also obtains the
  sender's signed confirmation. Both devices keep both ancestry heads.
- Range supports one closed, open-ended or suffix range. Downloads resume after
  interruption, reset if Range is ignored or returns 416, and retry hash
  failures. They stop after six attempts. No battery policy prevents transfer.
- Partial bytes use a private anonymous Linux file. They resume within retries,
  but are not retained across cancellation or daemon restart. This avoids adding
  an unapproved persisted partial-file layout. The durable repository keeps
  verified objects and records that already arrived.
- This module makes no writes to game save folders. It holds no session lock and
  does not itself implement first-sync choices, split UI, retention or restore.
- The object response is raw bytes as approved. Transport confidentiality follows
  the selected HTTP(S) endpoint; the content hash verifies integrity, not secrecy.

## Proof

Focused tests run two real peer HTTP routers with signed identities, verified
owner roster/endpoint evidence, separate temporary `NativeSaveStore`s, and the
same shared authentication function as production. They exchange divergent
children, keep both heads and the parent, verify all bytes, and inspect native
persisted confirmations. Other tests cover missing/corrupt objects, Range
resume, a response larger than the RPC envelope, replay, cross-owner refusal,
route separation, the snapshot push hook, and pre-launch deadline handling.
No in-memory save repository remains in these tests.
