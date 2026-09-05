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

## B5 owner-only peer snapshot wire

`services/korrid/src/lib.rs` owns `PeerListRequest`, `PeerListEntry`,
`PeerListState`, `PeerList`, and `PeerListOutcome`. Typeshare generates the
portal treaty. This is an RPC projection, not a new persisted schema.

| Wire element | Exact grounding |
| --- | --- |
| `app.peer.list`, empty `payload: {}` | Approved B5 PeerList operation; follows `app.local-games.list` and the existing empty request structs. |
| Response `outcome`, tagged `Ok` / `Err`, `payload` | Existing `RpcResponse`, `SourceStatusOutcome`, and `RpcFailure` envelopes in `lib.rs`. Success contains `PeerList.peers`, a collection like `LocalGames.games`. |
| `devicePublicKey` | `federation::PeerSnapshot.device_public_key`; the directory derives canonical keys from verified same-owner membership. Existing wire camelCase convention. |
| `label` | Existing effective registry order in `UpstreamRegistry::resolved`: configured static label when available, then `current_endpoint.or(remembered_endpoint).label`, then the full key. The endpoint label is signed metadata, not a name-service lookup. |
| `state` | Exact `federation::PeerState` cases: `loading`, `ready`, `failed`. Lowercase string enum follows the existing SourceStatus state enums. No endpoint or static URL is treated as proof of readiness. |
| `updatedAt` | Exact `PeerSnapshot.updated_at` local observation clock, in **Unix seconds**, not milliseconds, ISO text, endpoint issue time, or last-seen time. It is initialized on roster insertion / memory reopen and updated by native operation completion with a nondecreasing high-water mark. Rust uses checked `typeshare::U53` to preserve the existing integer seconds in JavaScript's `number`; an unrepresentable value fails the snapshot rather than rounding. |
| `lastError` | `PeerState::Failed.error`. The native candidate-operation producer already replaces raw transport / peer errors with `Authenticated peer operation failed`. Loading and ready omit the field with the established `Option`, `default`, `skip_serializing_if` convention. No address, signed evidence, pass, or storage path is added to the projection. |

Dispatch authorizes before reading one `FederationDirectory::snapshot()`. It
performs no network operation and does not refresh or mutate peer liveness.
The directory already excludes revoked keys and sorts by its `BTreeMap` key.
Static metadata is read through a narrow registry accessor without taking a
second directory snapshot. Host mode has no static upstream registry. Invalid
static configuration supplies no label override; it does not hide verified
peers. Directory validation or timestamp conversion failure returns the fixed
`PeerListUnavailable` / `peer directory unavailable` failure. A factory without
a directory returns an empty list; production host and brain composition keep
the shared directory from B4.

`OwnerDeviceOnly` permits authenticated same-owner devices. Scoped household
and guest passes, including passes with both catalog and stream scopes, and
unknown peers are denied. LocalBrowser still requires the listener's bearer
capability. LocalUnixControl keeps the existing private-listener authority.
Plaintext PeerList on `/peer-rpc` is rejected like every other plaintext call.

The portal adds `peerList()` to the HTTP and in-memory clients. HTTP checks the
response operation tag. Configured in-memory responses are cloned on input and
output; the default browser sandbox returns an empty list. There is no polling,
bridge event, surface change, or effect tied to the new method.

Cost and limit: a static-only peer has no directory liveness row in B3. It is
therefore absent until verified membership enters the directory; B5 does not
fabricate a loading row or add another registry. PeerList itself cannot change
loading to ready or detect an offline peer. A native catalog / source / session
operation must observe that result. Remembered labels remain usable after
endpoint expiry. Offline revocation remains delayed until verified evidence
arrives. Future state producers must preserve the existing error-sanitization
boundary before supplying `PeerState::Failed`.

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
are not accepted. B2 preserved the existing endpoint URL semantics. B3 adds the
strict origin validation and candidate failover described below.

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

Android resolves only the system-owned user-directory prefix before passing the
private root to Rust. An API 34 app process sees `/data/user/0` as a symlink to
`/data/data`, even when run-as sees a directory.
[Android 14 `ContextImpl.getNoBackupFilesDir()`](https://android.googlesource.com/platform/frameworks/base/+/android-14.0.0_r1/core/java/android/app/ContextImpl.java#849)
places the no-backup directory directly inside the app data directory. The shell resolves
that app directory's parent, then appends the unchanged app directory, no-backup
leaf and `korrid-state`. It does not canonicalize app-owned paths: Rust must still
reject symlinked app, no-backup, private-root and federation directories. This is
platform path resolution, not a second storage location or a permission repair.

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

## B3 authenticated native routing

`CoordinationSnapshot.endpoints` now carries `EndpointEvidence { record,
event_json }`. This is an in-memory receive result, not a new wire or persisted
schema. Feed `event_json` unchanged to `apply_endpoint_event` with the work token
captured before the relay read. The decoded record is not an insertion API.

`federation::peer_origin` is the shared Rust policy for static candidates,
received endpoint records and reloaded signed memory. It accepts HTTP(S) origins
of at most 2,048 bytes. It rejects credentials, whitespace/control characters,
backslashes, queries, fragments, non-root paths, an empty explicit port and port
zero. URL parsing enforces the port upper bound. Default ports, host case and a
single root slash normalize for candidate deduplication. Secure client constructors
retain raw candidates until this validation, before removing a root slash or
appending the RPC path. A raw double slash is rejected, not repaired. The public
constructor still returns a client; invalid input fails the operation before
network I/O. Relay WebSocket URLs keep their separate policy. No old endpoint
reader or runtime migration exists.

`UpstreamRegistry::with_federation` attaches the one shared directory to an
existing secure registry. Every operation composes the same effective set from
explicit configuration and verified directory snapshots. Device keys identify
peers. Static labels and Moonlight addresses override authenticated metadata;
without a label, a roster peer uses its full device key. A roster peer does not
need Moonlight metadata for catalog, source or session operations. Existing
static configuration errors still fail closed. Duplicate effective labels cannot
select a launch by first match; the existing label selector returns an error.
Selected remote routes and the mutation mutex remain shared across snapshots
and deferred static-file resolution.

The candidate order is static, eligible current endpoints, then remembered
endpoints, with canonical-origin duplicates removed without changing order.
B2 remembers one latest signed endpoint record, not an independent history of
old endpoint records. Current and remembered candidates can therefore be the
same list; expiry removes current eligibility without removing remembered use.
This slice adds no older-address history or new persistence fields.

`NativeClient` owns attempts beneath all typed operations. Each attempt signs a
fresh encrypted request and checks the expected responder key. One five-second
network-operation budget is divided among the remaining candidates. Catalog,
source status and session status can continue after transport or response
authentication failure. Authenticated application failures and HTTP 4xx failures
are terminal. No request changes to plaintext or follows a redirect.

Prepare, stop, freeze, thaw and all certificate operations retry only a failure
classified by reqwest as connection establishment failure. A lost/incomplete
response, timeout, HTTP response or wrong-key response is ambiguous after send
and is never replayed. No idempotency or retry wire fields were added. Cost:
an operation can have taken effect even when its caller receives an error.
Session recovery can inspect an existing session; an error is not proof that a
mutation had no effect. The per-candidate budget can also end a slow mutation
before the total five seconds have elapsed.

A directory peer operation captures its B2 per-peer token and commits Ready or
Failed only after the candidate sequence and typed response check finish.
Failures use fixed local text, not remote bodies, URLs or credentials. Equal
outer native timeouts were removed so they cannot cancel the final state update.
Superseded completions cannot replace a newer live state. The shared Authorization
revocation query also applies to static peers with no roster row, before each
attempt and again at completion. Static overrides cannot restore a revoked key.
Registry fan-out also rechecks each native identity when consuming buffered
results, after all peers finish. Revoked catalog results become host failures;
revoked recovery results make recovery incomplete; revoked certificate results
cannot attest a match. This adds authorization reads and cannot retract results
already delivered to callers.

Verified terminal revocation retires only the exact selected device key and
launch ID through compare-and-clear. Prepare keeps its existing mutation lock
and can then select another authorized peer. Status, stop, freeze and thaw that
encounter the revoked selection return `SourcePeerNotFound`, not a successful
remote stop. Later status describes only the remaining controlled sessions.
Disappearance, configuration changes, transport failures and authorization-read
errors do not retire the selection. Cost: Korri relinquishes control of the old
launch; remote execution may continue. Retirement does not send a stop to a
revoked peer and does not prove that execution stopped.

Brain construction now opens the directory with its existing credential clone
and one authorization authority. B4 must retain these same objects for discovery
and identity reload. A future Linux composition with inbound peer RPC must also
pass that authorization instance to the RPC server, not load another cache.
No coordinator, publication task or lifecycle hook is started by B3. Private
roots must already satisfy the B2 rules; runtime code does not chmod or repair
existing state. This adds startup failure on nonprivate or invalid memory rather
than silently using an empty directory.

## B4 discovery and lifecycle

`federation::coordinator::FederationResources::open` composes one credentials
mutex, one Authorization and one directory for a private root. Production brain
registries and Linux inbound peer RPC receive these same clones. `AppState`
retains the actual directory for later snapshot consumers. The coordinator
creates its short-lived relay signer from the current credentials snapshot on
each cycle; it does not load an independent signing identity.

Discovery runs immediately. Successful or partially successful relay observations
poll again after 60 seconds. Total operation failure retries after 5 seconds,
doubles the delay, and stops increasing at 5 minutes. An empty successful read
is not a relay outage. `CoordinationSnapshot.all_relay_reads_failed` carries that
transport distinction explicitly; signer packet reads keep their old contract.
Global relay failure does not mark reachable remembered peers Failed. Native
operations remain the authority for peer Ready/Failed state.

Each cycle publishes the stored, person-signed owner event unchanged, reads the
same-owner roster, and applies both latest and terminal-revocation signed
sources with the directory work token. It then captures a fresh token before
reading endpoint ciphertext and commits that original evidence. A bounded relay
snapshot is never an instruction to delete absent peers.

A nonempty local advertisement reserves its generation and NIP timestamp before
any endpoint publication. Both EndpointRecord issue time and the outer event
use that timestamp. Lifetime is 24 hours; renewal is after 6 hours. Startup and
changed publication inputs make a new reservation. Per-recipient successful
generations are held in memory. A newly observed owned recipient gets the
current generation in that cycle, without waiting for renewal. Partial writes
remain pending and retry on the next poll; total write failure uses backoff.
A crash burns a reservation and restart makes a newer one. No publication state
or recipient-delivery schema was added to the private document.

Linux keeps `KORRID_RELAYS`. `KORRID_ADVERTISED_ENDPOINTS` is a JSON array of
strict HTTP(S) origins; its default is empty and valid query-only operation.
`KORRID_MOONLIGHT_ADDRESS` is optional. The reloaded readable `host.title` supplies
the authenticated label; the NixOS service supplies standard `HOSTNAME` from
`networking.hostName` as the fallback. Both existing Nix module interfaces expose
`advertisedEndpoints` (default `[]`) and nullable `moonlightAddress`. There is no
new readable config field or label option. WebSocket rules apply only to relays,
not peer endpoints.

Android explicitly reloads the existing config coordinator and checks its
SnapshotAuthorization before reading `host.relays`. Android publishes its owner
event but never constructs an empty endpoint record, because the embedded brain
has no reachable peer listener. Successful settings updates wake discovery;
ordinary polling also reloads readable configuration. After local owner binding
is durably applied, the original shared credentials reload and the directory's
work epoch advances before waking discovery. Port, browser capability and
WebView lifetime do not change.

A wake cancels the current asynchronous cycle before fresh signing/relay work.
Shutdown cancels and joins discovery, including blocked WebSocket handshakes,
reads and publications. Android owns the task inside its Tokio runtime and joins
it before the server thread exits. Stop releases the server-slot mutex before
the blocking thread join. Linux joins after SIGTERM, SIGINT or either serving
surface fails. Tokio's signal feature supplies Linux signal handling; no second
signal runtime or polling signal handler was introduced.

Cost and limits: a new unremembered device needs a working relay or explicit
static configuration. Offline revocations remain delayed indefinitely; remembered
endpoints do not prove current membership freshness or reachability. Relays can
reject delegated person-authored EVENT publication even when device NIP-42 auth
works. There is no second roster protocol or plaintext fallback. Cancellation
bounds asynchronous network work, not synchronous signature verification or
private-filesystem operations already in progress. Public-relay policy, real
wireless routing and Android physical lifecycle behavior remain acceptance work.

## B6 emulator environment and acceptance (2026-09-05)

The ordered discovery/session/remembered-restart acceptance passed on two fresh
API 34 `google_apis` x86_64 AVDs. The six native bridge tests passed between them,
serially. Neither B6 run used `upstreams.json`. Both retained binding authority,
verified the encrypted relay/native-peer path, completed exactly one play,
restarted Android with the relay stopped, and recovered the same host after an
observed outage. Each helper trace contains exactly one launch, freeze, thaw and
stop. Full `korrid-check` also passed.

Fresh online AVDs were not a stable fixture: GMS Chimera changed its module
configuration, killed `com.google.android.gms.persistent`, and Android killed
Korri because it depended on GMS FontsProvider (exit reason 12). Android's
`device_config set_sync_disabled_for_tests persistent` did **not** stop Chimera:
the owned-AVD experiment verified that setting, then observed module-change
restarts. [Android 14 SettingsProvider](https://android.googlesource.com/platform/frameworks/base/+/android-14.0.0_r1/packages/SettingsProvider/src/com/android/providers/settings/SettingsProvider.java#1171)
gates `setAllConfigSettings`; this is not a control for GMS's private Chimera
module configuration.

The shared emulator harness now starts a rejecting loopback proxy before boot
and supplies the emulator's official `-http-proxy` transport option. Its own
`-help-http-proxy` documents redirection of all guest TCP connections. The proxy
never forwards a request. The harness waits at most 30 seconds for an actual
TEST-NET connection to reach that proxy, then verifies an HTTP response over ADB
reverse. It checks the owned AVD name, lock, process and explicit serial before
probing. No Android setting, package, font or essential dependency is disabled.
A host-side FIFO keeps the reverse probe's stdin open until its response; ADB
reverse does not preserve the guest TCP half-close. No initialization sleep or
instrumentation retry is used.

All three final runs retained the same GMS persistent PID through instrumentation
and reported no Chimera module restart or Korri dependency death. GMS remained
enabled; Chimera logged actual network sync errors rather than updated modules.
Evidence: `/tmp/federation-isolation-b6-first.log`,
`/tmp/federation-isolation-bridge.log`,
`/tmp/federation-isolation-b6-fresh-repeat.log`; retained Android diagnostics are
`/tmp/korri-android-evidence.Ad7HvnvNWH`, `YIyW8p9Nr4` and `grbjLqsRoh` under the
same prefix. `/tmp/federation-emulator-stability-fix.md` records causal evidence,
fixture corrections, full-gate results and the earlier failed setup experiments.

A newly reached session assertion also exposed a fixture-only environment bug:
the executable systemd helper has a Nix shebang, but its isolated environment
omitted `NIX_PATH`. The helper could not resolve `nixpkgs`, so real session
recovery correctly failed closed. The fixture now includes this toolchain
variable alongside `PATH`; a test executes the actual helper with that exact
environment. No production session behavior or lifecycle assertion changed.

Cost: these are isolated loopback federation/bridge fixtures, **not Google-services
integration tests**. External TCP, including downloadable fonts and Google module
updates, is unavailable. The emulator proxy does not isolate UDP/DNS. This is not
a general network-security sandbox, a public-relay proof, or a physical-device
result. GMS sync continues to attempt work; an image with a different bootstrap
transport may need a reviewed harness change. Run
`clients/android/test/emulator-bootstrap-check.sh` for proxy and ownership guards.
