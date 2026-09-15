---
date: 2026-09-15
topic: plugin-model
artifact: brief
status: direction agreed; detailed design preserved for review; not implemented
supersedes: conflicting plugin-model proposals in the September 9 authoring brief and the temporary aso examples
---

# Plugin model: implementation briefing

## Purpose and authority

Preserve the plugin-model exploration in Git before implementation starts.
The user approved the direction, asked for a final/full review design on aso,
then explicitly required **no backward compatibility**. This change is
**documentation only**. It does not authorize starting the implementation.

The durable design is [2026-09-15-plugin-model/](2026-09-15-plugin-model/README.md).
It was copied from `aso:~/korri-plugin-design/`, the folder the user reviewed.
The older `plugin-remodel`, `korri-plugin`, and `korri-data-code-example`
folders on aso are historical experiments, not implementation specifications.

Read this brief first. It records the final clean-cut instruction and the
repository constraints that control implementation. The copied design
separates agreed direction from detailed recommendations; do not promote
those recommendations into settled requirements merely because they appear
in a TypeScript or YAML example.

This brief supersedes the September 9 authoring brief where it conflicts on
`plugin.nix`, launcher/core separation, Korri-level `requires`, and shared
family settings. It does not revoke unrelated decisions about signing,
cache publication, native service artifacts, platform boundaries, or
prebuilt device delivery. The older status documents remain historical.

## Problem

The current model separates launcher and core and carries Korri-specific
inter-plugin dependency/lifecycle work. The user wants one authoring model:
plugins fill Korri seams; common emulator cores are easy to generate; a
full DIY plugin uses the same contract; first-party support is not a special
runtime privilege.

The exploration briefly considered YAML declarations and a two-function TS
ABI. Reviewing all 50 legacy plugin folders showed the latter was too
narrow: discovery, acquisition, mutable preparation, launch composition,
live controls and cleanup are real needs. TS remains the authoring format.
The operation inventory is a coverage guide, not a mandate to implement a
25-operation framework before a real consumer needs it.

## Decisions to retain

1. **One plugin model.** All plugins have equal standing. A plugin can fill
   any supported seams; it need not provide a runner.
2. **Source layout.** An independent source plugin has `flake.nix` and a
   lock file. `plugin.ts` is the entry when source is needed. Generated
   plugins do not need separate source folders or handwritten TS entries.
3. **Ordinary imports.** Authors can split source and use shared TS libraries.
   Node/Bun compatibility and dynamic-import support are not required now.
4. **Runner seam.** Collapse launcher plus libretro-core selection into a
   runner. Do not confuse that with deleting CPU-translation, native-runtime
   readiness, or streaming capabilities.
5. **Dependencies remain Nix dependencies.** Remove Korri-level `requires`
   walking and superseded lifecycle/approval machinery. Do not remove Nix
   dependencies, source regressions, or runtime closure references.
6. **Optional scaffolding.** A shared plugin can publish a family, Nix helpers,
   and a TS library. A core author can use them or write a full DIY plugin.
   A normal source module keeps the same contract when a helper fills omissions.
7. **Separate generated plugins.** One catalogue/flake can emit one installable
   output per core, with independent identity, receipt and enablement.
8. **Versions coexist.** Runners retain the native program/library versions
   they were built with. Identical Nix paths deduplicate; different versions
   sit side by side. There is no runtime substitution by an installed family.
9. **Scoped shared settings.** Family settings are defaults. No extra locking
   system without a demonstrated need. Names and units stay stable;
   unsupported settings are reported. A matching family ID is not proof of
   semantic compatibility.
10. **Naming.** Public configuration uses the nouns `families`, `runners`,
    `devices`, and similar sections; no `by-` prefix. User setting names use
    kebab-case. Native settings retain their native spelling.
11. **Composition order.** Explicit, never installation order or plugin-name
    order. Show the resulting plan and intermediate transformations for diagnosis.
12. **Session ownership.** Korri owns the session. Plugins return plans or
    tracked resource handles. Cleanup runs after normal exit and failed startup.
13. **Escape hatches.** Retain the existing legacy design; do not replace it
    with a freshly invented generic extraConfig mechanism. Typed support can
    lag behind native program versions.
14. **Proportionate trust.** Keep useful validation and resource ownership,
    not a new attestation product. Signatures do not prove behavior is safe.
15. **No device builds.** Publish prebuilt closures. RetroArch is not added to
    the base image as a shortcut. Runtime TS preparation remains supported.

## Detailed recommendations still to settle

These are in the review design so they can be examined together. They are
not already deployed or proven requirements:

- The exact two-axis cascade, including system versus runner specificity,
  profile/preset placement, and null/array behavior. Write conflict cases
  before implementing the resolver. Preserve legacy field-specific override
  accumulation rather than applying a generic merge to every field.
- Rename the frontend-specific shared integration to `@korri:retroarch`.
  The user delegated that naming call; the recommendation reflects the fact
  that the vocabulary and renderer are RetroArch-specific, not libretro API
  features. This does not introduce runtime aliases for the earlier draft ID.
- The final static manifest/source inventory and operation schemas. The
  copied examples are proposed shapes, not an existing wire treaty.
- Settings-contract version semantics and persistent save/save-state identity.
- The precise effect boundary for richer operations. `HostServices` in the
  review type file records needs found in legacy; it is not authorization to
  inject arbitrary I/O into the JS sandbox. Keep Korri in control of effects
  and federation-capable placement, as `AGENTS.md` requires.

## Existing sources to use

- `AGENTS.md`: clean-cut policy, thin plugins, native artifacts, effect ownership,
  source execution, no builds on target devices, and Rust-owned wire types.
- `services/korrid/src/script.rs`, `script/preparation.rs`, `script/source.rs`:
  actual TS/module support. The engine supports prepared graphs; the package
  admission path observed during review still registers only `plugin.ts`.
- `services/korrid/plugin-host/`: current packaging, native-unit policy,
  approval, lifecycle, builder tests and VM checks.
- `services/korrid/src/config/` and `src/launcher/`: current resolution and effects.
- `contracts/generated/`: Rust/Typeshare output; never edit it by hand.
- `legacy` commit `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62`: read-only behavioral
  reference, not a branch to merge. The retained escape-hatch sources are
  cited precisely in [ESCAPE-HATCHES.md](2026-09-15-plugin-model/ESCAPE-HATCHES.md).

The design's implementation observations were made against local main
`0a30ad4ef2afca75f46d87a79344b07ac1a72978`. This documentation branch is based on
published main `f3aa66d9`; it deliberately does not publish the 11 unrelated
local device commits. Recheck current source before implementation.

## Landing strategy: one clean cut

Use one implementation worktree and one coordinated contract-cutover PR.
Atomic commits help review; they must not create a transition architecture.

1. Settle the remaining detailed choices against existing producers/consumers
   and explicit user decisions. Keep a decision trace rather than inventing
   schema to make a sample look complete.
2. Implement artifact/source-graph support and the selected operation contract.
   Keep wire types in Rust and regenerate TS through the existing workflow.
3. Replace the connected declaration → resolution → cascade → preparation →
   session → portal path in the same branch. Update plugin producers, fixtures,
   tests and clients together.
4. Prove generated mGBA/Snes9x, an explicit RetroArch override, DIY PPSSPP,
   Tailscale, and one session-owned integration such as Gamescope.
5. Delete superseded code, fields, approval/dependency logic, fixtures and docs
   as their replacements are written. Do not retain them until some later
   migration phase.
6. Verify and merge the complete cut. Existing artifacts/configuration that
   do not satisfy the new contract are unsupported, not interpreted through
   aliases or fallback reads.

**Forbidden:** backward-compatibility adapters, aliases, fallback reads,
dual writes, automatic runtime migrations, and parallel old/new contract
support. Different native program revisions coexisting in Nix is dependency
isolation, not old Korri contract compatibility.

Preserve actual saves and user data. If deployed configuration needs
conversion, use an explicit one-off operator action outside the runtime.
Do not delete real user data to make the new schema appear clean.

## Verification gates for implementation

Tests were deferred while discussing the design; they are required before
merging the runtime replacement.

- Unit/contract tests for declarations, operation bindings, source graphs,
  every chosen cascade conflict, legacy escape-hatch semantics, and final
  launch plans.
- Real core/frontend package builds and native configuration checks. The
  earlier `.opt` via `--appendconfig` smoke example is not correct proof of
  core-option delivery and must not be copied as an implementation.
- Native-unit and session lifecycle checks, including partial-start failure,
  cancellation, ordered composition, and cleanup that preserves saves.
- Relevant Rust formatting/clippy/tests, generated contracts, portal/surface
  checks, Android contract/build checks, and Nix formatting/layout checks.
- Hardware acceptance only on an available, authorized device. Do not claim
  Android or Linux hardware behavior from host-only tests.
- Publish signed closures. RG DS deployment remains SD-only, no device
  compilation, and `boot` plus reboot rather than live switch.

## Current delivery and next action

This delivery preserves the briefing and review documents/examples. It
changes no runtime, plugin, contract generator, device image or device state.
Syntax/type checks on illustrative examples are not runtime acceptance.

The next engineering task is to turn the selected design details into a
small set of executable contract/cascade cases and then implement the clean
cut in an isolated worktree. Do not start that task as part of this docs save.
