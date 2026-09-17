# Installed Linux routes

The approved cascade is implemented in `config/cascade.rs`. Its order is
runner → system → runner → game → override. Each stored layer uses
`families.<family-id> and runners.<full-runner-id>`. Kinds do not contribute configuration.
Executable runner records come only from installed plugin contributions.
`device.yaml` runner records now contain configuration only. Old executable
runner records require an explicit operational cutover, not a fallback reader.

## Portal API

Use the local korrid tagged `/rpc` treaty in `contracts/generated/korrid.ts`.
These three methods apply to installed Linux games. Android and peer launch
methods are unchanged. No surface or chooser UI is implemented here.

| Method | Payload | Success |
|---|---|---|
| `app.local-games.routes` | `GameRoutesRequest { gameId }` | `GameRoutes`: full runner/runner/kind IDs, exact runner/runner package paths, program path, warnings, selection, stored game/system IDs and document revisions. |
| `app.local-games.runner.set` | `GameRunnerSetRequest { scope, runnerId?, expectedRevision }` | Updated `RunnerChoiceRevisions`. Scope is `{ _tag: "Game" or "System", id }`. Omit `runnerId` to clear that scope. Use `revisions.games` for a game and `revisions.device` for a system. |
| `app.local-games.launch.selected` | `SelectedGameLaunchRequest { gameId, runnerId, overrides? }` | `{ session: SessionPrepared, warnings: LaunchWarning[] }`. Uses the existing unprivileged session executor. Does not save a preference. |

Responses retain the existing `{ _tag: method, outcome: { _tag: "Ok" or
"Err", payload } }` envelope. Write conflicts return `SettingsConflict`;
reload before retrying. Writes use the existing validated, revision-checked
atomic YAML writer. They preserve other record fields and refuse a pending
discovery publication.

A game choice overrides a system choice. An explicit launch overrides both.
A stale stored ID remains stored and produces `selection._tag = "Choose"`,
even with only one available candidate. An unchosen game with one candidate
selects it automatically. With no playable installed candidates, route listing
returns the existing route diagnostic. Builds are package paths, not versions
embedded in IDs. Several claims for the same system produce one scanned file
candidate; claims that disagree on system still need system identification.
This preserves the release record's existing single-system contract.

Route reads are available to read-only portals. Explicit launch also works
with `LocalSessions`. Preference writes require `Full`, like existing settings
writes. Peer requests for these local actions require an owner device. This
slice does not expand household/guest or read-only mutation permissions.

Explicit selected launch returns `ActiveSessionConflict` if a session is
already active, including the same game. The existing recovery record has no
runner identity, so reusing it could falsely report that the selected runner
ran. Stop that exact session before switching routes. Ordinary
`app.session.prepare` retains its existing same-game resume behavior.

## Source-checked rendered settings

`runner::typed_settings::validate(settings, runner_id, build,
Option<SourceCheckedSettings>)` accepts only matching keys and scalar types.
Its borrowed evidence uses the exact launching instance package path as
`build`, the pinned program display `version`, and a `keys` map with
`SettingType::{Boolean, Number, String}`. There is no `since` table.

The `korri-plugins` repository owns the producer in
`plugins/retroarch/settings-check.nix` and `check-settings.py`. It checks the
kind's `settings-types.json` against the
supplied instance program's patched `configuration.c`. The shared builder and
Rust core do not parse RetroArch source. Other instances call this check with
their own program derivation, never with the kind's default program.

Derived metadata uses the existing manifest `packages` and `files` maps. The
file key is `<program file key>-settings`. Its three fields are grounded in
existing consumers: `program` is `PluginLaunchInput.program`; `version` and
`keys` are the `SourceCheckedSettings` fields above. `PackagedSettings` reads
only the selected instance's file, verifies the exact executable path, then
binds the evidence to that installed package's path. This avoids a Nix package
referring to its own output. No user configuration schema or manifest field
was added. Missing or mismatched evidence admits no settings; malformed
artifacts fail the route rather than becoming authority.

Grounding: `legacy:product/plugins/retroarch/src/policy.ts` owns the nested
RetroArch policy. The `korri-plugins` repository preserves that schema in
`plugins/retroarch/policy.ts`.
The pure `renderRetroArchSettings` code is extracted from legacy
`launch-spec.ts` into `render-settings.ts`. Its scalar outputs define the
kind's key/type table. A schema-driven test checks every fixed output against
that table. `legacy:product/platform/library/config/launch-block.ts` defines
shallow per-key last-wins merging; the existing cascade is unchanged.

The scalar callback seam is **rendered output**, not a replacement flat user
policy. The callback renders accepted pairs before raw config, preserving
legacy boolean/number/string quoting. Nested policy persistence and entry into
the host remain outside this seam; this change does not invent that contract.
Dynamic per-port settings currently lack evidence and are omitted with warnings.

Warnings name the setting, runner, exact instance build, and pinned version
(or explicitly unavailable metadata). Route listing and selected launch expose
`LaunchWarning[]`; the ordinary catalog uses `LaunchSettingUnsupported`.
These are the existing UI warning seams; no UI or warning treaty changed.

The check proves source key/type recognition, not that every conditional
upstream feature was compiled in. Unsupported or type-conflicting legacy keys
are omitted, not renamed or coerced. See
`korri-plugins/plugins/retroarch/README.md` for the current source result and
focused verification commands.

Raw `LaunchOverrides.config.prepend/append` retains the existing legacy
contract: scalar fields merge last-wins, both render after Korri's lines, and
append follows prepend. RetroArch still rejects replace and reserved keys in
its approved callback. This slice adds no callback-output validation.

`typeshare.toml` only teaches the generator how to print Serde's untagged
scalar union. It is not a device configuration schema. Regenerate with
`nix run .#korrid-check -- --types-only`.
