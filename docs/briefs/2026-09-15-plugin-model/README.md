# Korri plugin design — consolidated review

This is the version-controlled copy of `aso:~/korri-plugin-design/`.
Read the [implementation briefing](../2026-09-15-plugin-model-brief.md) first.
It records the later **no-backward-compatibility** instruction and the
repository rules that control implementation. Where it qualifies a proposal
in this captured design, the briefing wins.

**This is the current design for review, not a completed implementation.**
It consolidates the conversation and the audit of all 50 legacy plugin folders.
It supersedes the earlier `plugin-remodel`, `korri-plugin`, and
`korri-data-code-example` proposals for review purposes. Those folders are
left unchanged as history; do not treat their code as this specification.

Read in order:

1. **DESIGN.md** — package model, files, families, generation, and ownership.
2. **CASCADE.md** — exact proposed precedence and worked conflicts.
3. **ESCAPE-HATCHES.md** — the existing legacy rules, retained explicitly.
4. **OPERATIONS.md** — complete proposed operation inventory and lifecycle.
5. **plugin-contract.ts** — typed review draft for declarations and operations.
6. **examples/** — a core catalogue, a per-core addition, user settings, and
   a native-version override. Illustrative source, not ready-to-build flakes.
7. **IMPLEMENTATION.md** — current support, remaining work, and later tests.

## Decision status

**Agreed:** TS authoring; ordinary `flake.nix`; optional `plugin.ts` entry;
ordinary imports; equal treatment of all plugins; optional shared libraries;
separate plugins generated from a catalogue; Nix dependencies retained;
versions coexist; family settings are defaults; stable setting names/units;
unsupported values are reported; explicit wrapper order; Korri-owned
sessions and cleanup; native escape hatches; proportionate trust model;
testing after design review.

**Recommended details for approval:** the precise two-axis cascade; scope
expansion of profiles/presets; generic clear semantics; the operation names
and detailed types; the generated manifest/source inventory; settings
compatibility versions; saved-data identity; rename the current shared
integration to `@korri:retroarch`. These are marked in their sections.
They were requested as design calls, not silently recorded as prior approval.

**Retained from legacy:** `launch.overrides.args/config` with
`prepend`, `append`, `replace`; separate fold and application rules; adapter-
specific native config handling. See ESCAPE-HATCHES.md for source citations.

No new global locking system. No universal `extraConfig` replacement for
legacy escape hatches. No runtime dependency walker. No requirement to
compile the plugin to JS before shipping. No requirement for a YAML plugin
file. No promise of Node/Bun APIs.
