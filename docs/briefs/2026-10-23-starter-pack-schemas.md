# Starter-pack schema proposals

**Proposed, not approved.** This brief does not change a contract or authorize
implementation. It covers #192, #193, #194 and #197, under #189 and ADR 0033.

Core citations refer to `299166068095982bbed0c7600f1e97b8480605d0`.
`korri-plugins:` citations refer to the separate publisher repository at
`5aff7ee954851713eaf87316ccc46a3e8fe61f08`.
`legacy:` citations refer to `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62`.
Line ranges are from those revisions, not the estimates in the issues.

The terms **Release**, **Declared game**, **Starter pack** and **Artifact**
keep their meanings in `CONTEXT.md:162-184`. A list is an offer, not a declared
game or an artifact. Accepting it leaves ordinary games and plugins. It leaves
no pack-membership record or claim on later updates and removals.

## #192 — Lists offered by a plugin

### List grounding and delta

| Existing or legacy element | Evidence | Proposed delta and reason |
| --- | --- | --- |
| The pack's `games`, release `hashes` and `file.packageFile` | `korri-plugins:plugins/starter-pack/plugin.ts:5-16`; `docs/adr/0016-declared-games-can-come-from-their-own-package.md:11-37` | Replace package-owned games in this pack with references to provider items. ADR 0033 requires independent removal. Do not change the general declared-game schema. |
| Exact runner requirements | `korri-plugins:plugins/starter-pack/plugin.nix:12-18` | Replace this pack's `requires` with runner plugin IDs in its list. Do not weaken exact `requires` elsewhere. |
| Provider item reference | `services/korrid/src/config/library.rs:159-178`; `legacy:product/platform/protocol/acquisition/claim.ts:45-59` | Reuse its meaning. Spell a list reference `providerId` and `id`, as the acquire consumer already does in `services/korrid/src/acquisition/types.rs:153-167`. Do not rename persisted `provider`, `playable` or `refs`. |
| Provider claim, details and download result | `contracts/generated/korrid.ts:1136-1158`, `contracts/generated/korrid.ts:16`; `korri-plugins:plugins/pico8/plugin.ts:75-122,162-205` | Leave these types and PICO-8 handlers unchanged. A list reference is not a search claim or a download URL. |
| Pure provider-plugin handlers | `services/korrid/src/acquisition/turns.rs:12-14,86-145,337-392`; `services/korrid/SCRIPTING.md:465-490` | Add a list operation beside this handler mechanism. A list plugin must not acquire content itself. |

### Proposed shape and operation

Propose the new handler name **`starter-packs.list`**. Its input is an empty
record. Its result is an ordered array of list records. Each record has:

| Field | Meaning and grounding |
| --- | --- |
| `id` | A contribution ID in the offering plugin's namespace. Reuse the `<pluginId>/<key>` grammar in `docs/adr/0012-korrid-acquires-declared-artifacts.md:18-20`. |
| `title` | The list's review label. Reuse `ProviderRecord.title` from `services/korrid/src/plugin.rs:16-29`, with the non-empty rule at `services/korrid/src/plugin.rs:1093-1102`. |
| `games` | An ordered array of references, each with `providerId`, `id` and required `expectedHash`. The first two fields feed ordinary acquire. The new `expectedHash` field is defined in #193 below. This array replaces this pack's existing `games` declarations; it does not declare library games. |
| `runners` | An ordered array of runner **plugin ID strings**, not runner contribution IDs or store paths. These are the identities of the plugins currently named by this pack's `requires`. ADR 0033 explicitly chooses plugin IDs (`docs/adr/0033-a-starter-pack-is-a-list-acquired-once.md:13-18`). |

The list record and its `games` and `runners` containers are new schema, even
though their names already exist. The handler name is also new. These are
proposals, not recovered legacy list syntax. Reject unknown fields, malformed
references and conflicting hashes for the same provider item. Reuse current
provider-ID and item-ID admission, not a second ID grammar.

Use the existing `handlers` export. An installed, approved and enabled plugin
that offers this operation can supply lists. A list-only plugin need not export
`providers`: today that export requires all three acquisition handlers
(`services/korrid/src/plugin.rs:2204-2234`). This is an explicit extension of
handler discovery, not permission to weaken that existing provider contract.
The list handler returns local data only. It does not return `HttpRequests`.
Keep the sandbox deadline, heap and output limits
(`services/korrid/SCRIPTING.md:543-545`).

The host checks each referenced provider before acquire. Missing or unapproved
providers make that entry unavailable, even if bytes exist. The host reviews
each missing runner through its own existing inspection and approval. It then
uses ordinary per-item acquire and install operations. Setup's current
enable-and-rescan flow changes, not its persisted installed flag: there is no
such flag (`clients/portal/src/surface/starter-pack-onboarding.ts:16-23,64-100`).
Running a list again asks first and skips what is present. No background
reconciliation reinstalls removals.

### List choices and costs

| Choice | Recommendation or open question | Alternative and cost |
| --- | --- | --- |
| Register and expose lists to the portal | Approve handler-presence discovery and the list shape first. The korrid listing RPC and its exact request are still unresolved. | A new named declaration export makes registration explicit, but adds another contract. Do not invent it without that choice. |
| Review labels without network | Provider references alone contain neither game titles nor systems. Resolve the offline metadata question in #193 before implementing fresh setup. | Embed claim details in each list entry. This reuses a type, but duplicates provider metadata in the list and changes the proposed three-field entry. |
| Select a missing runner's release | Reuse existing catalog selection and exact inspection. Which real catalog selection should define the default offer remains open. | Pin runner outputs in the list. This restores the coupling that #189 exposed. A plugin ID alone cannot authorize a release or permissions. |
| Remote or person-entered lists | Keep them out of this slice. | URLs need network and trust rules; files need another admission path. Both exceed the local list producer in ADR 0033. |
| Three non-PICO-8 games | Wait for real providers, as ADR 0033 requires at `docs/adr/0033-a-starter-pack-is-a-list-acquired-once.md:42-43`. | A package-file exception preserves pack ownership and gives these games different removal rules. |

## #193 — Expected content hash and offline cache recognition

### Acquire grounding and delta

| Existing or legacy element | Evidence | Proposed delta and reason |
| --- | --- | --- |
| `AcquireArtifactRequest` | `services/korrid/src/acquisition/types.rs:153-167`; `contracts/generated/korrid.ts:62-70` | Add optional **`expectedHash`**. Ordinary store callers can omit it; every list game supplies it. The request currently has no expected digest. |
| Legacy expected digest set | `legacy:product/platform/protocol/acquisition/artifact-acquisition.ts:48-70`; `legacy:product/platform/protocol/artifact/artifact.ts:58-69` | Do not restore `expectedDigests`. That was plugin-acquire output, not this request. A single release hash is the chosen consumer need; multiple algorithms have no current producer here. |
| Release content identity | `services/korrid/src/acquisition/game.rs:714-736`; `services/korrid/src/config/library.rs:122-129`; `docs/adr/0013-declared-games-can-come-from-artifacts.md:35-39` | Reuse `sha256:<64 lowercase hex characters>`. Compare content identity after the system's header rule, not an archive or transport digest. |
| Provider cache and its one ready file | `services/korrid/src/acquisition/placement.rs:226-232,268-306,309-323` | Keep `<storage-root>/cache/<plugin>/<provider>/<sha256 hex>/<URL basename>`. Add hash-checked lookup before a provider call. No new path, sidecar readiness flag or URL scheme. |
| Existing offline owned-game shortcut | `services/korrid/src/acquisition/game.rs:321-382,508-518`; `services/korrid/SCRIPTING.md:657-668` | Extend beyond an existing provider link. A pre-filled folder can exist before the game has joined the library. |

### Proposed behavior

`expectedHash` is a **new field**, shared by the list reference and the acquire
request. Reject explicit null and malformed hashes at admission. It constrains
bytes; it does not supply metadata, a URL, a path or provider approval.

1. Require the installed and approved provider. Derive its cache from the
   registry's owner, not from caller-supplied paths.
2. For a request with `expectedHash`, inspect that provider's matching ready
   folder before `claims.details` or `artifact.resolve-download`. Require one
   regular, non-hidden file, with no symlink escape. Apply the enabled system's
   content rules and compute its release identity. Folder naming alone is not
   proof. A wrong file fails; do not import it or silently use different bytes.
3. Once metadata is available, reuse `record_acquired`, under its existing
   publication lock, and return ordinary `Imported { gameId }`. Keep the
   library item, release and provider-link shapes unchanged
   (`services/korrid/src/discovery/reconcile.rs:37-44,409-428`).
4. On a cache miss, obtain ordinary provider details, then resolve and download.
   Compare the computed release identity with `expectedHash` **before**
   `publish_acquired_game` and `record_acquired`. A mismatch fails the item.
   Reuse `Failed.reason: RpcFailure`; the exact mismatch code remains open.
5. Both the owned-game shortcut and live-job reuse must honor the expectation.
   Today they key by provider item only. Never return a job for different or
   unchecked bytes as proof that this request matched. Whether to refuse a
   conflicting concurrent expectation or queue it remains open.

Omitting the field retains ordinary store semantics. This is a new request
constraint, not a compatibility read. It does not add job persistence or
resume. HTTP(S), host approval and SSRF rules stay unchanged.

### Acquire choices and costs

**Fresh offline setup is not fully specified by a hash and a folder.** The
current writer needs `title` and `system`, and the held claim also needs `url`
(`services/korrid/src/acquisition/game.rs:46-52,194-202,371-400,691-736`). A new
image has no held claim or provider link. Calling PICO-8 details needs HTTP
(`korri-plugins:plugins/pico8/plugin.ts:88-95`). Do not claim that cache lookup
alone meets #193's fresh-device acceptance.

| Choice | Recommendation or open question | Alternative and cost |
| --- | --- | --- |
| Durable offline metadata | Which real provider-details output or image record should define it? Prefer reusing admitted `ProviderClaimDetails` unchanged if the owner chooses retained provider output. Its storage and admission source remain unresolved; no filename or new wrapper is proposed here. | Seed ordinary library records before acceptance: no new record shape, but games appear before the offer is accepted. Embed details in the list: changes #192 and duplicates provider metadata. Network-only details: cannot meet offline first boot. |
| Pre-fill bytes | Use the existing publication layout and verify at import. Image packaging must supply the same main content bytes and content rules. | Trust the folder name or build check alone: faster, but fails the wrong-cached-file acceptance. Hashing costs local I/O. |
| Multi-file content | Into Ruins has a companion cartridge, explicitly not a separate game (`korri-plugins:plugins/starter-pack/plugin.ts:5-8`). Which real provider record should define acquisition of that companion? The current ready-file consumer accepts one file only. | Treat the companion as another game: false library identity. Add a multi-file schema now: unsupported by the current provider contract. Do neither silently. |
| Verification failure | Fail without writing a library record. Exact error code and repair action need approval. | Automatic deletion and redownload hide damaged pre-fill and need network. Accepting a newer provider file breaks the list's fixed expectation. |

## #194 — Remove a runner plugin or an acquired game

### Removal grounding and delta

| Existing or legacy element | Evidence | Proposed delta and reason |
| --- | --- | --- |
| Tagged admin requests and empty applied result | `services/korrid/src/plugin_admin.rs:165-169,203-231,233-249`; `services/korrid/plugin-host/src/admin.rs:53-90,116-171` | Add **`Remove`**, with a new `PluginRemoveRequest` containing only existing identity field `id`. Reuse `Applied(PluginChangeApplied)` and `Refused(PluginAdminFailure)`. |
| Host removal and durable removed intent | `services/korrid/plugin-host/src/host.rs:1168-1194`; `services/korrid/plugin-host/src/selection.rs:30-36` | Call existing `Host::remove(id, false)`. Do not expose `purge`, arbitrary paths or cleanup commands to the portal. Ordinary removal retains private data and saves. |
| Plugin RPC forwarding | `services/korrid/src/lib.rs:1394-1407,3695-3705` | Add **`system.plugins.remove`**, using the same `PluginRemoveRequest` and `PluginChangeOutcome`. A runner is a plugin removal, not a second runner API. |
| Acquired-game removal | `services/korrid/src/acquisition/types.rs:81-87`; `services/korrid/SCRIPTING.md:729-734` | Reuse `app.acquisition.remove { gameId }`. No new game-removal field or request. Retain history, shared content and saves. |
| Fail-closed lifecycle authority | `services/korrid/plugin-host/src/lifecycle.rs:4-6,27-67`; `services/korrid/plugin-host/src/main.rs:287-304` | Production currently supplies `removal_unavailable()`. A product-authoritative caller must supply the existing `RequiredPlugin { id, behavior }` view before removal can work. Do not replace missing authority with an empty list. |
| Local portal permission | `services/korrid/src/portal_access.rs:100-143` | Add only plugin removal to `LocalSessions`. **Game removal is already allowed** at line 123. The issue's suggested game-permission expansion is already present at this revision. |

The new request uses `id`, not a runner contribution ID, a catalog URL or a
release. The existing receipt supplies selection and provenance. Confirmation
lists affected games before the action. korrid must recheck live and pending
launches before forwarding removal, including every runner in a multi-runner
plugin. Refuse in-use removal; do not end a game implicitly. Serialize that
check with launch admission so a launch cannot start between check and removal.
Withdraw the removed plugin's launch authority before admitting another launch.

Keep socket peer-UID checks, message bounds, host locking, dependency guards
and exact approved cleanup. The privileged host remains responsible for
required-plugin refusal. It must not trust a browser's statement that a plugin
is optional. Removal never deletes saves or stops their sync.

### Removal choices and costs

| Choice | Recommendation or open question | Alternative and cost |
| --- | --- | --- |
| Authoritative product policy | Which existing product/release caller will supply `LifecyclePolicy` to the production admin host? This remains a blocker. The current policy is an in-memory input, not a persisted schema (`services/korrid/plugin-host/README.md:181-185`). | Assume all runners are optional, or seed an empty policy: permits removal of required behavior. Persist requiredness in plugin receipts: contradicts the existing authority rule. |
| Affected-game review | Derive it from current routes. The review response shape and launch/removal exclusion mechanism remain open. | Save a runner-to-game membership list: duplicates route state and becomes stale. Confirm with no affected-game list: fails ADR 0033. |
| Removal scope | First cover acquired games and runner plugins. Copied source files need a separate ownership decision. | Delete arbitrary source files through this API: expands authority into person-owned folders, which ADR 0013 refuses. |
| Local authority | Security review must verify local capability/origin enforcement, concurrent launches, pending cleanup and required dependencies. `ReadOnly` must not gain removal. | Grant all `system.plugins.*` actions to `LocalSessions`: also grants installs and repository changes, outside #194. |
| Purge | Keep it absent from this new wire. | Add the host's `purge` boolean: grants private-state deletion and risks saves. A confirmation boolean is not a security boundary. |

## #197 — Minimum durable target for whole-graph rollback

### Rollback grounding and delta

| Existing or legacy element | Evidence | Proposed delta and reason |
| --- | --- | --- |
| One previous exact selection | `services/korrid/plugin-host/src/selection.rs:8-28,38-77` | Keep `package`, `provenance` and `approval`. Extend the root's `Previous` to retain the graph that its approval depended on. Independent child `previous` slots can advance and cannot identify one root's prior graph. |
| Exact dependency reports and bound approvals | `services/korrid/plugin-host/src/package.rs:30-53,672-707,727-763` | Reuse the dependency IDs and exact selection tuples from `brings`. Keep each dependency's own provenance; never copy the root's source into its children. |
| Same-ID replacement refusal | `services/korrid/plugin-host/src/host.rs:703-736`; observed failed FAKE-08 and pack updates in #189 | Permit reviewed replacements within the selected root's graph. Do not bypass guards for another installed dependent, even a disabled one. |
| Single-plugin rollback | `services/korrid/plugin-host/src/host.rs:868-889` | Load the root's retained graph target, rather than swapping each child's current `previous` independently. |
| No second graph journal today | `services/korrid/plugin-host/README.md:93-109` | Propose a root-receipt extension, not a separate authoritative graph file. This changes the README's no-new-receipt-schema boundary. It does not make native effects atomic. |

### Proposed minimum record

Steam declares exact FEX and Proton requirements
(`korri-plugins:plugins/steam/plugin.nix:1-17`). Propose the first slice for
replacing these three selections when all are already installed. Keep graph
membership and plugin IDs unchanged in this slice.

Propose two new fields on the root's existing `Previous`:

- **`desired`**: the root's pre-transition `Receipt.desired`.
- **`brings`**: a dependency-first array of member snapshots. Each member uses
  the existing `Receipt` fields `id`, `package`, `provenance`, `approval` and
  `desired`. Do not copy a member's own `previous`, or recursively copy graphs.

Both names already exist, but these placements are **new fields**. The root's
existing previous tuple and this flat array form one rollback target. The
saved `desired` values preserve enabled/disabled intent before temporary
quiescence. The consumer is whole-graph rollback and recovery; this is not a
new dependency declaration. Edges and order come from retained immutable
manifests, not another saved edge list.

Capture the complete target before changing the first member. Never rebuild it
from child receipts after an update has started. Pin all retained outputs,
including outputs no longer reachable from the new root. Existing per-node
`previous` roots do not prove that these snapshots remain pinned
(`services/korrid/plugin-host/src/selection.rs:81-118`). Check provenance,
approvals, publisher authority and exact manifests before any restart.

On update, the existing inspection binds each new member's own approval into
the root approval. Review once. Stop dependents first, apply dependencies
first, and publish only a consistent graph. Keep refusal for a dependency
shared with another root at a conflicting exact build. Rollback uses the saved
root and member tuples together and leaves unrelated plugins untouched.

### Rollback choices and costs

**The snapshot is a rollback target, not a complete crash protocol.** Per-file
atomic writes (`services/korrid/plugin-host/src/storage.rs:141-171`) cannot
commit several receipts together. The current `pending` roots mark individual
work, not which root owns a graph transition. Do not implement graph updates
from this snapshot proposal alone.

| Choice | Recommendation or open question | Alternative and cost |
| --- | --- | --- |
| In-flight graph intent and commit point | Decide how the root receipt identifies the prepared target and the selected graph across a crash. Test interruption before and after every receipt/root write and native start. The marker's exact schema is unresolved; the snapshot above does not invent a phase field. | A separate graph journal can make recovery explicit, but supersedes the README rule and creates another authority unless per-plugin receipts become projections. Only per-plugin `Previous` keeps the old bug: a partial update has no common rollback target. |
| New or dropped members | Keep the first proposal limited to replacement of existing IDs. Which real changed-membership graph should define absence and retention? | Snapshot nullable receipts, as `SelectionStore::read` already returns (`services/korrid/plugin-host/src/selection.rs:94-96`): can represent a previously absent member, but removal, sharing and rollback policy need a real case. Infer absence from a missing file: confuses absence with damage. |
| Preserve rollback intent | Propose the saved `desired` fields. Reject unfinished `Removed` members during preflight. | Preserve only current intent, like today's leaf rollback: fewer fields, but failed quiescence can lose the graph's prior enabled state. |
| Bounds and retained software | Reuse graph bounds of 128 plugins and 1024 edges. Check the existing 128 KiB receipt bound before effects (`services/korrid/plugin-host/src/storage.rs:17,141-145`). Root retention rules need tests against GC. | Raise limits or retain many generations: more disk and validation cost, without evidence from this three-plugin case. |
| Existing receipts | An approved schema change needs an explicit one-off operational conversion. Its exact cut depends on the chosen crash protocol. | Runtime fallback reads or default-empty graphs conceal missing history and violate `AGENTS.md`. Do not add them. |

Required later checks are root-plus-two-dependency update, failed native start,
whole-graph rollback, revoked candidate, crash recovery and an untouched
unrelated plugin. No behavior or deployment is claimed by this document.
