# Implementation status and later verification

The user asked for a consolidated design before more implementation and
explicitly deferred testing of the full system. No new plugin API is claimed
to be implemented by this folder.

## Existing foundations

- Nix store paths/closures and signed cache delivery.
- Oxc in-process TS preparation and QuickJS evaluation.
- A prepared module graph engine, including tested package-resolution paths.
- Legacy static contributions, operation handlers, shared host services,
  discovery observations, lifecycle hooks, and native package wrappers.
- Legacy escape-hatch fold/application code and regression tests.

## Gaps to implement

1. One final serialized declaration/operation schema and matching Rust/TS
   types. Choose the generation direction once; neither the old CONTRACT.ts
   nor this review file silently makes TS the sole source of truth.
2. Package source graph production and admission. Today package_plugin
   registers only plugin.ts despite the engine's broader graph support.
3. The helper/builder API shown in examples. The GitHub input names are
   illustrative locations, not verified published packages.
4. Static extraction and generated manifest validation; no duplicate authored
   metadata; keep named artifacts separate from module-source entries.
5. Typed host operations/services, jobs, resource ownership and session dispatch.
   The review contract defines useful boundaries, not every native RPC protocol.
6. Exact cascade implementation and provenance, including scope-aware profile
   expansion and field-specific legacy escape-hatch folding.
7. Family settings compatibility and family UI/control contributions without
   changing the runner's bundled defaults/code when another version is installed.
8. Persistent-data mapping/migration and per-session generation retention.

## Explicitly not part of this design

- New runtime plugin-to-plugin requires/install cascades.
- A plugin-language-specific package class.
- Mandatory JS bundling or handwritten manifest JSON.
- Automatic family schema-superset attestation or a general locking framework.
- Global native settings that every game must understand.
- Node/Bun compatibility or dynamic imports in the first implementation.
- Restoring every legacy implementation detail without deciding whether the
  user-visible behavior is still needed.

## Later verification matrix

| Case | What must be demonstrated |
|---|---|
| Generated mGBA + Snes9x | Separate identities/receipts; shared paths dedup; core filenames correct |
| mGBA frontend override | Different frontend coexists; correct config and native parser behavior |
| Full DIY runner | No family helper; same contract and lifecycle |
| Family absent/present/updated | Standalone runner works; settings and controls remain compatible; no code substitution |
| Settings conflict | All CASCADE.md examples produce explainable values |
| Escape hatch | Legacy args slots and config semantics preserved; unknown native keys not rejected by stale typed schema |
| Core options | Correct .opt/core-options interface, not --appendconfig misuse |
| Imports | Relative/package imports packaged and admitted; no hidden build-machine files |
| Discovery/provider | File/folder/provider targets; no lexicographic default or implicit installs |
| Mutable installation | Readiness, cancellation, progress, and partial-failure recovery |
| Gamescope/CDP/remap | Explicit wrapper order, readiness, control readback, normal and failed-start cleanup |
| Updates/removal | Active generation retained; saves/settings survive; incompatible states not overwritten |
| Diagnostics | Final plan/provenance visible without credential leakage |

## Review-only checks for this delivery

The type contract and illustrative TS are checked for TypeScript syntax/type
consistency, and Nix snippets are formatted and parsed. Local references and
escape-hatch citations are reviewed. These checks do not build the example
plugins, validate native option meanings, or prove operation compatibility
with korrid.

From the repository root, the review-only type check is:

```sh
nix-shell -p typescript --run \
  'tsc -p docs/briefs/2026-09-15-plugin-model/tsconfig.json'
```

The local tsconfig excludes ambient Node/Bun types. These examples use only
the review contract and standard ECMAScript types; unrelated repository
node_modules must not become accidental API dependencies.

## Legacy audit coverage

All 50 folders were inspected at legacy commit
0e4cec9da3d77e6578b8a01a5d83420ba0d98e62. The full coverage inventory is in
`evidence/legacy-coverage.md`. Native implementations, acquisition, and
session-owned code informed this design; simple declarations were not counted
as proof of simple runtime behavior.
