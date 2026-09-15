# Operation contract — proposed v1 review inventory

These names and signatures are the recommended design, not current exports
in korrid. They cover the behavior classes found in the legacy audit. Not
every plugin implements them. Do not keep adding aliases for the same role.
`plugin-contract.ts` defines the request/result structures for this draft.

## 1. One call convention

```ts
export const handlers = {
  "launch.prepare": async context => { /* return a launch plan */ },
  "launch.compose": context => { /* return a modified plan */ },
}
```

Every handler receives `{ input, settings, files, context, services }`:

- input: operation-specific validated request;
- settings: resolved settings for this invocation, not a mutable config store;
- files: named immutable artifacts from THIS plugin generation;
- context: request/session/device/account facts;
- services: granted host capabilities and host-owned resource allocation.

A handler returns a value or a promise. It may throw/reject with a structured
plugin error. Pure functions need no services. Native helpers, HTTP, content
reads and mutable preparation use explicit host services where needed.
Node/Bun globals are not promised. Native executables remain available as
packaged helpers, not plugins recompiled on the handheld.

The host identifies the operation from the map key; there is no duplicated
YAML function-name binding. Generated manifests list the available operation
names. Build helpers can import ordinary TS and produce the same map.

## 2. Operations

| Operation | Request | Result and ownership |
|---|---|---|
| preferences.map | General preferences plus selected runner | Native setting contribution, handled keys, diagnostics; no hidden final layer |
| settings.describe | Runner and content context | Runtime schema fragment/choices with revision; never code or identity replacement |
| settings.options | Setting path and runner context | Labeled values with revision and optional expiry |
| settings.validate | Effective values and runtime context | Diagnostics; no silent rewriting of authored values |
| discovery.scan | Supplied file evidence and source context | Candidate observations; Korri owns identity/reconciliation and YAML writes |
| catalog.list | Cursor/filter | Static or runtime catalog page |
| claims.search | Query/filter/cursor | Provider claims; not installed games |
| claims.details | Claim reference | Details and available artifacts |
| claims.parse-url | URL | Provider-owned reference or no match |
| provider.validate | Provider/account context | Ready/unavailable/user-action status; no implicit login prompt at module load |
| artifact.resolve-download | Artifact reference | Final transfer request or explicit user-action/unsupported result |
| artifact.acquire | Approved resolved transfer | Artifact handle or job; host owns storage, byte limits and progress |
| install.request | Acquired artifact and destination intent | Install job or explicit user-action; not an implicit launch side effect |
| job.status | Job handle | Progress/result/diagnostics |
| job.cancel | Job handle | Cancellation acknowledgement; subsequent status shows actual cleanup |
| runtime.resolve | Runner and target requirements | Availability, selected immutable references and missing preparation; not launching |
| launch.prepare | Selected runner, target, launch facts, folded legacy overrides | Base LaunchPlan, generated files, diagnostics |
| launch.compose | Existing plan and modifier settings | New plan preserving host/session ownership; no implicit chaining |
| session.started | Host process/session handles and final plan | Readiness and owned resource handles, or error |
| session.describe | Session and runner | Currently available controls and values |
| session.control | Advertised action/value and session | Applied/rejected/unsupported outcome, with readback where meaningful |
| session.stopping | Session and stop reason | Stop-before-cleanup work; host still ensures termination |
| session.cleanup | Session, including partial startup, and reason | Released/residual resources and diagnostics; idempotent |
| stream.discover | Source context and cursor | Stream endpoints/capabilities; no forced connection |
| diagnostics.collect | Plugin/session context | Structured status, not unrestricted secret-bearing dumps |

Streaming launch preparation uses launch.prepare. A persistent transport
control connection is owned by the session/native service, not stored in a
JSON declaration. CPU translation/runtime resolution is not removed merely
because launcher+libretro-core became one runner.

Fixed launch declarations are a shortcut interpreted by the host. They do
not force every game plugin to export a trivial function. Static extension
rules are also a shortcut; discovery.scan handles manifests/folders/content
where an extension alone is insufficient.

## 3. Settings and runtime values

Static schemas use data, with named keys, units, ranges and enums. A runtime
settings description supplements them when the application supplies options
only at runtime. Host validation checks the returned schema fragment.
A revision identifies which set of choices was shown. Validate the selected
value again before launch if the device/session state could have changed.
Cache results by plugin generation and relevant device/account/target context,
not globally. Report stale selections clearly. Dynamic description cannot
supply new executable imports, a different plugin identity, or new grants.

The shared TS type uses JSON Schema 2020-12 with local references only.
Production validates schemas and values; it does not fetch remote schemas. JSON Schema syntax is not a replacement for
cross-field/runtime validation. settings.validate covers checks such as
CDP release threshold < press threshold. A value rejected by validation is
reported, not silently changed or deleted from saved config.

Unsupported values and native escape-hatch values are different. A typed
field can be unsupported for this runner version while native text remains
available. The renderer must not invent support, and the user can choose a
native override or a different package generation.

## 4. Jobs, handles and side effects

Acquisition/installation can outlive one call. The host creates job IDs and
tracks progress, cancellation and results. job.status/job.cancel apply to
both acquisition and installation jobs; they are not installation-only. Plugin code can use helper
processes or services; their identity/lifetime is registered with the host.
A returned resource handle is a reference to a host-tracked resource, not
a serialized closure or a plugin-generated authority token.

Declare which host APIs a plugin needs. Treat these as install-time access
requests under the trusted-plugin model. No approval dialog per file read.
Content reads are scoped to supplied evidence; HTTP requests use explicit
service access; native helpers come from named package artifacts. Exact
native helper protocols can be plugin-owned and validated by its TS library;
Korri does not need to know Steam VDF or RetroArch config syntax.

## 5. Launch and cleanup sequence

1. Resolve target and candidate runners. Ambiguous discovery does not choose
   an emulator alphabetically. Use the person's selection or ask.
2. Resolve settings and any dynamic options. Preserve unsupported values.
3. Check runtime/content readiness. Offer installation/preparation explicitly
   if needed; do not run `nix run` or compile on the device.
4. Call launch.prepare (or use a fixed declaration).
5. Call selected launch.compose handlers in explicit list order.
6. Validate and show the final plan. Host writes planned files atomically,
   acquires tracked resources, and spawns the process.
7. Invoke relevant session.started hooks. Required readiness failure stops
   the launch; optional integration failure is reported. The contribution
   declares whether its startup hook is required or optional.
8. Offer only controls the active runner/session supports.
9. On exit, failure or cancellation, invoke stopping/cleanup as applicable,
   release host resources in reverse allocation order, and retain persistent
   saves. Generic cleanup does not depend on plugin cleanup succeeding.

A failure during step 6 still releases resources acquired before it.
All resource allocations through host services are recorded immediately,
not only when a plugin returns successfully.

## 6. Diagnostic contract

Every diagnostic has a code, severity, readable message and optional field
path. Do not log credential values. The host adds plugin/generation/request
identity. Handler errors include whether retry could help; cancellation is
separate from an internal failure. Unsupported operations are explicit,
not interpreted as successful empty results.

The resolved-plan view includes settings provenance and native override
sources, every wrapper step, the selected package versions, and the final
executable/argv. Sensitive values are redacted. This is troubleshooting for
an entertainment system, not a new audit/attestation product.
