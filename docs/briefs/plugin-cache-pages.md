# Plugin cache metadata on GitHub Pages

The approved cutover moves the three public plugin metadata indexes to Pages.
NAR payloads remain in their existing GitHub Releases. Signing keys, package
paths, permission checks and the existing Receipt schema stay unchanged.

| Publisher | Metadata cache |
|---|---|
| `@korri` | `https://korri-os.github.io/plugins/` |
| `@simonwjackson` | `https://simonwjackson.github.io/korri-plugins/` |
| `@miniarray` | `https://miniarray.github.io/korri-plugins/` |

The publisher appends metadata on `cache-index`, deploys that branch through
Pages, and waits for exact-byte visibility. Old `cache` releases stay frozen
and readable. They do not receive new metadata. Existing NAR URLs and signed
metadata are copied byte for byte during the initial move.

`release::batch_url` now maps a canonical Pages project URL to the same
repository's `build-<commit12>` release. It still requires a full lowercase
40-character revision. Batch evidence only locates candidates. Inspection
still verifies the package, source, closure and full publisher key. No runtime
alias to the old URL is provided.

## Existing devices

Changing an exact publisher binding denies activation under old raw-cache
receipts even if the signing key stays the same. Ordinary updates do not solve
this for installed dependency graphs because dependency discovery retains old
receipt provenance and installed dependents veto individual replacement.

The one-off `nix/cache/migrate-plugin-receipts.py` uses existing producers:

- `services/korrid/plugin-host/src/selection.rs` defines Receipt and Previous.
- `src/package.rs` defines PublisherBindings and graph loading.
- `korri-plugin seed-graph` computes complete approvals and enabled states.
- `src/storage.rs` defines the host's exclusive `lock` file.

The script reads settled raw-cache receipts under that lock, verifies each
selected package with its existing full publisher key, checks old graph
approvals, and prepares before/after Receipt files. New approvals come from
`seed-graph` with the new bindings. It preserves package paths, data, desired
states, previous selections and GC roots. It introduces no runtime migration
or persisted host schema.

The prepared backup contains only the existing Receipt and PublisherBindings
formats in ordinary before/after directories. Apply requires every changed
selection's exact approval digest and the reviewed active bindings. It checks
the complete live snapshot before writing. Repeating apply or rollback can
finish a partial replacement. Unknown live changes, pending roots, unfinished
graph updates, stale approvals and key rotation cause refusal.

Preparation and live cutover are separate. First prepare against an offline
copy of the device state with its exact package closure available on the build
machine. Present new digests for owner approval. Stop the portal, korrid and
plugin admin/restore services during the live replacement. Take another exact
snapshot before test activation. Keep a separate SSH recovery path available:
a plugin-provided SSH or network service can stop when its authorization changes.

Apply reviewed receipts only after the approved binding change. Run the normal
host restore, then verify every previously enabled plugin, korrid and the
portal. A failure restores both original receipt bytes and the old system
binding. Reboot alone is not a receipt rollback. Keep the snapshot outside the
plugin state directory and retain both system generations until acceptance.

The first device is RG35XX Pro. Each later device needs separate approval.
Mini V2 and Odin personal `file://` bindings remain unchanged until selected
outputs have approved public sources. Never publish their private cache as a
shortcut. Public metadata migration does not clear the rights to a plugin or
to game content.

## Verification

`checks.x86_64-linux.korri-plugin-cache-cutover` uses the real host graph
producer, a separate Nix database and real signed packages. It covers mixed
enabled/disabled dependency graphs, exact approvals, interrupted apply,
idempotent apply, exact-byte rollback, changed-live-state refusal and pending
operations. Host release tests cover all three Pages URLs, canonical URL
checks, exact revisions, and unchanged batch-path validation.

No script in this change builds on a device, weakens publisher authorization,
starts a device update automatically, or writes firmware.
