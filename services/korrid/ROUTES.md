# Installed Linux routes

The approved cascade is implemented in `config/cascade.rs`. Its order is
launcher → system → runtime → game → override. Each stored layer uses
`launchers.<full-launcher-id>`. Kinds do not contribute configuration.
Executable runtime records come only from installed plugin contributions.
`device.yaml` runtime records now contain configuration only. Old executable
runtime records require an explicit operational cutover, not a fallback reader.

## Portal API

Use the local korrid tagged `/rpc` treaty in `contracts/generated/korrid.ts`.
These three methods apply to installed Linux games. Android and peer launch
methods are unchanged. No surface or chooser UI is implemented here.

| Method | Payload | Success |
|---|---|---|
| `app.local-games.routes` | `GameRoutesRequest { gameId }` | `GameRoutes`: full runtime/launcher/kind IDs, exact runtime/launcher package paths, program path, warnings, selection, stored game/system IDs and document revisions. |
| `app.local-games.runtime.set` | `GameRuntimeSetRequest { scope, runtimeId?, expectedRevision }` | Updated `RuntimeChoiceRevisions`. Scope is `{ _tag: "Game" or "System", id }`. Omit `runtimeId` to clear that scope. Use `revisions.games` for a game and `revisions.device` for a system. |
| `app.local-games.launch.selected` | `SelectedGameLaunchRequest { gameId, runtimeId, overrides? }` | `{ session: SessionPrepared, warnings: LaunchWarning[] }`. Uses the existing unprivileged session executor. Does not save a preference. |

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
runtime identity, so reusing it could falsely report that the selected runtime
ran. Stop that exact session before switching routes. Ordinary
`app.session.prepare` retains its existing same-game resume behavior.

## Typed metadata producer handoff

The producer is not implemented in this slice. No metadata path, manifest field,
version table, or `since` table has been invented.

The smallest seam is `launcher::typed_settings::validate(settings, launcher_id,
build, Option<SourceCheckedSettings>)`. The borrowed evidence contains:

- `build`: the exact launching instance package path, not its kind package;
- `version`: the pinned program's display version;
- `keys`: a map from rendered setting key to `SettingType::{Boolean, Number,
  String}`.

Grounding: `legacy:product/plugins/retroarch/src/policy.ts` owns the scalar
`LaunchSettingValue` union and the nested RetroArch policy. Its
`launch-spec.ts::renderRetroArchSettings` produces scalar cfg pairs.
`legacy:product/platform/library/config/launch-block.ts::mergeLaunchSettings`
defines shallow per-key last-wins merging. The current scalar seam represents
those renderer outputs; it does not claim to port the complete nested policy.
The parent must preserve that policy when adding its producer and renderer.

The kind must check keys and types against the instance's pinned
`configuration.c`, then supply the evidence at the current `None` call site
in `launcher/linux_plugin.rs`. Evidence for another build accepts no settings.
The callback input is `PluginLaunchInput.overrides.settings`; the kind's
renderer must consume accepted pairs before raw config. The current RetroArch
callback has no typed-pair renderer yet. That belongs with the producer slice.

Until then, every requested typed setting is omitted, not silently accepted.
Warnings name the setting, launcher, exact build, and unavailable version
metadata. Route listing and selected launch expose warnings directly. The
ordinary catalog exposes them as `LaunchSettingUnsupported` diagnostics.
This is a safe incomplete producer boundary, not a completed typed policy.

Raw `LaunchOverrides.config.prepend/append` retains the existing legacy
contract: scalar fields merge last-wins, both render after Korri's lines, and
append follows prepend. RetroArch still rejects replace and reserved keys in
its approved callback. This slice adds no callback-output validation.

`typeshare.toml` only teaches the generator how to print Serde's untagged
scalar union. It is not a device configuration schema. Regenerate with
`nix run .#korrid-check -- --types-only`.
