---
id: 01M2NF33QVDXQ4EZ3D43E7CY11
slug: complete-the-plugin-operation-contract-21-of-25-operations-r
title: "Complete the plugin operation contract: 21 of 25 operations remain"
origin: parked
status: To Do
priority: high
labels:
  - plugins
  - operations
  - epic
created: 2026-09-16
source: se-work
context:
  branch: feat/plugin-operations
  repo: korri
  invoked_by: user
---

# Complete the plugin operation contract: 21 of 25 operations remain

## Why it matters

The operation inventory lives only in a design document and in one session transcript. The design lists 25 operations. Four exist: launch.prepare, settings.describe, settings.validate, runtime.resolve. The other 21 are the difference between a launcher for files that are already on the device and the system legacy had. A legacy audit at commit 0e4cec9d shows 21 of 50 legacy plugin folders declare at least one operation that does not exist today, across 129 declaration sites. Without one record of the list, the state, and the blocker for each operation, the next session must measure all of this again.

## Acceptance Criteria

- [ ] Every operation in the list has one of three states: built with a test, or refused in writing with a reason, or blocked on a named missing host service
- [ ] No operation is built without a real caller in the tree
- [ ] A person can search this one item and find the operation list, the blocker for each operation, and the legacy evidence, without reading a session transcript

## Related

- `docs/briefs/2026-09-15-plugin-model/OPERATIONS.md`
- `docs/briefs/2026-09-15-plugin-model/plugin-contract.ts`
- `docs/briefs/2026-09-15-plugin-model/evidence/legacy-coverage.md`
- `services/korrid/src/script.rs`
- `services/korrid/SCRIPTING.md`
- `services/korrid/src/launcher/typed_settings.rs`
- `plugins/libretro/retroarch.ts`

## Notes

STATE, at branch feat/plugin-operations (3 commits above feat/plugin-model-clean-cut).

The call convention exists. A plugin exports `handlers`, a map from operation
name to handler function. The host names the operation. An unimplemented
operation is a typed case (script::OperationFailure::Unimplemented), never a
successful empty answer. Adding an operation adds a key, not a new export
contract. See services/korrid/src/script.rs and SCRIPTING.md.

=== THE 25 OPERATIONS ===

BUILT (4)
  launch.prepare      Runner returns a launch plan. Proven by the libretro cores.
  settings.describe   Runner returns its own schema fragment plus a revision.
  settings.validate   Runner reports values this build cannot apply.
  runtime.resolve     Runner reports whether it can start this target, and what is
                      missing if not. launcher/linux_plugin.rs calls it before a
                      launch starts; a refusal reaches the person as
                      LocalRouteUnavailable. Six unit tests in launcher/runtime.rs
                      and six integration tests in tests/runtime_resolve.rs.
                      The result carries `ready` and `missing` only: the draft's
                      `files` and `diagnostics` have no consumer yet.

NOT BUILT (21), with the blocker for each:

Settings group
  settings.options    No runner supplies options only at run time. No caller.
  preferences.map     korrid parses LaunchVideoPreferences and LaunchAudioPreferences
                      (config/mod.rs:688-720) and resolves them nowhere. The
                      ConfigSnapshot the cascade reads carries no preferences at
                      any scope. Build the resolved path first; the operation is
                      small after that. Rule: explicit native settings at the
                      same scope win over the translated preference (CASCADE.md
                      section 2). Report an unsupported preference; never invent
                      support and never drop it in silence.

Modifier group
  launch.compose      No modifier plugin exists on main, so there is nothing to
                      order. 10 legacy sites: gamescope, box64-runtime, remap,
                      turnip. Apply modifiers in the explicit list order of the
                      design, never installation order and never name order.
                      Show the resolved plan for diagnosis.

Session group (5)
  session.started
  session.describe
  session.control
  session.stopping
  session.cleanup     All five need the half korrid performs: reach a running
                      process over a protocol the host does not know. Today that
                      behaviour is the closed Rust enum SessionControlEffect,
                      wired to Android-only executors in
                      materialize_session_controls_snapshot (lib.rs:1840).
                      Session 3ca8942f was rewriting those files. Coordinate.
                      3 legacy sites for session.cleanup: gamescope, steam.
                      Korri must keep the session and run cleanup after both a
                      normal exit and a failed start. Generic cleanup must not
                      depend on plugin cleanup.

Discovery group
  discovery.scan      The static fileReleases extension map covers every file in
                      the repository today. Needed for content an extension
                      alone cannot identify: manifests, folders, contained
                      playables. Korri keeps identity, reconciliation and the
                      YAML writes.

Content group (10)
  catalog.list        No legacy equivalent. New design.
  claims.search       10 legacy sites, 9 plugins: itchio, pico8, smwcentral,
  claims.details      levelsharesquare, portmaster, mega-man-maker, smbxgame,
  claims.parse-url    community-catalog, acquisition-fixtures.
  provider.validate   9 legacy sites.
  artifact.resolve-download  10 legacy sites.
  artifact.acquire    4 legacy sites. Needs host storage and byte limits.
  install.request     Steam only in legacy. Needs a host job service.
  job.status          No legacy equivalent; legacy used install.status (Steam).
  job.cancel          No legacy equivalent.
                      The whole group needs host services that do not exist:
                      storage, byte limits, progress, job identity, HTTP.

Reporting group
  stream.discover     The one possible caller is app.moonlight.resolve, which
                      belongs to the Moonlight extraction work.
  diagnostics.collect 23 legacy sites, the most used legacy operation. The host
                      has no decided report seam. Structured status only; never
                      an unrestricted dump that can carry a secret.

=== SIX OPERATIONS ARE NEW DESIGN, NOT LOST FUNCTION ===
catalog.list, job.status, job.cancel, discovery.scan, settings.options and
preferences.map have no legacy equivalent. Do not count them as recovery.

=== LEGACY NAMES WITH NO EQUIVALENT IN THIS DESIGN ===
portmaster.install (8 sites), portmaster.prepare-launch (13 sites),
gmloader.install, gmloader.launch.path.prepare (5 sites),
gmloader.payload.inspect, gmloader.prepare-launch, lifecycle.collect,
lifecycle.correlate, package.expose, cli.expose, stream.launch (5 sites),
stream-control.describe, stream-control.connect, stream-control.apply.
Plugin-specific operation names were normal in legacy. PortMaster and GMLoader
need more than a rename.

=== RECOMMENDED ORDER ===
1. launch.compose, diagnostics.collect. With runtime.resolve built, these two
   cover the rest of the 43 legacy declaration sites and need no new host
   storage or job service. Every legacy runtime plugin paired runtime.resolve
   with launch.compose, and launch.compose is where legacy actually applied the
   runtime environment.
2. The session group, after the native control seam is decided.
3. preferences.map, after the resolved preference path exists.
4. The content group, after host storage, byte limits and jobs exist.

=== RULES THAT HOLD FOR EVERY OPERATION ===
- A plugin performs no effect. It returns a declaration; korrid performs it.
- The host names the operation. A plugin never selects its own handler.
- An unimplemented operation is explicit, never an empty success.
- Korri owns the session, storage, byte limits, progress and cleanup.
- Ground every request and result field in the design contract or an existing
  producer or consumer. Do not invent schema ahead of a real case.
- One operation group per atomic commit, with tests for each group.

=== SOURCES ===
docs/briefs/2026-09-15-plugin-model/OPERATIONS.md   the 25-operation table
docs/briefs/2026-09-15-plugin-model/plugin-contract.ts  typed draft, Operations map
docs/briefs/2026-09-15-plugin-model/evidence/legacy-coverage.md  50 legacy folders
Legacy commit 0e4cec9da3d77e6578b8a01a5d83420ba0d98e62, branch `legacy`.
Count legacy use again with:
  git grep -h -oE 'operation: "[a-z.-]+"' legacy -- 'product/plugins/*' | sort | uniq -c | sort -rn

=== runtime.resolve, MEASURED 2026-09-17 ===
The operation is built. Two facts the next person should not remeasure:

- Legacy declared it and never invoked it. `git grep -n 'runtime.resolve'
  legacy` finds only the operation union in product/platform/plugin/index.ts,
  the four plugin.ts handlers, their tests, and six game plugins' `requires`.
  There is no caller: legacy has launch-prepare.ts and launch-companion.ts, and
  no runtime-resolve.ts. The env the resolvers returned (BOX64_PREFER_EMULATED,
  FEX_ROOTFS, VK_ICD_FILENAMES, the Proton paths) was also produced by
  launch.compose, and that is the path legacy wired. So the "10 legacy sites"
  are 4 provider declarations plus 6 test declarations, not 10 callers.
- No producer exists on main. There is no box64, fex, or proton plugin in
  services/, plugins/, or contracts/, and the new model has no `requires` or
  capability declaration for content to demand one. The operation therefore has
  a real caller (launcher/linux_plugin.rs) and a tested seam, but its first
  first-party producer arrives with the runtime plugins. Port one runtime plugin
  and prove it answers, rather than adding a handler that always says ready.

The six legacy plugins that require the capability are the real demand:
mega-man-arena, mega-man-maker, midas-machine, portmaster, psycho-waluigi,
smb-wonderland-1987.
