# Portal → person CLI parity

Audit base: `a8cc7f528` (ADR 0031). Commands below start with `korri`.
This is an inventory of real portal producers, not a proposed settings schema.
Dynamic IDs are shown as prefixes or with the producer's placeholders.
A command still obeys korrid's permissions, revisions, route availability and
local-only boundaries. A mapping does not promise remote authorization.

## Settings rows

Source paths in this section are under `clients/portal/src/surface/`.
`settings-model.ts` publishes the device rows; `use-launchables.ts` routes their
changes. `launch-settings-model.ts` publishes launch rows; `launch-settings.ts`
routes them. `plugin-repositories.ts` and `plugin-repositories-controller.ts`
publish and route plugin rows.

| Portal setting ID / action | CLI command | Result / limits |
| --- | --- | --- |
| `device-name` | `devices rename NAME` | Same `SettingsUpdate` revision |
| `person-name` | `identity name NAME` | Same person-name producer and signer requirement |
| `setup-restart` | `setup restart` | Retains games, identity and Wi-Fi |
| `host.preferences.playerCount` | `controllers seats COUNT` | Configured seats, not physical assignments |
| `steamgriddb-credential` set / clear | `metadata key set --key-stdin` / `metadata key clear` | Private input; never show key |
| `local-games` (read-only) | `library games list` | Actual local game IDs |
| `game-discovery-status` (read-only) | `library folders list` | Scan state and diagnostics |
| `library-folder-rescan` | `library folders rescan` | Same rescan |
| `library-folder:ID` / `library-folder-remove:ID` | `library folders remove ID --yes` | Unregisters; does not delete game files |
| `korrid-version` (read-only) | `status` | Running daemon version; `version` is the CLI version |
| `plugin-repository-add` | `plugins repositories add URL` | Same repository validation |
| `plugin-repository:official`, `plugin-repository:URL`, their `:status` rows | `plugins repositories list`; `plugins list URL` | Read-only official repository and catalog status |
| `plugin-repository-remove:URL` | `plugins repositories remove URL --yes` | Installed plugins stay |
| `plugin-release:TARGET` / `plugin-inspect:TARGET` | `plugins review URL PLUGIN VERSION` | Same exact release inspection |
| `plugin-review:TARGET` | `plugins install URL PLUGIN VERSION --approval DIGEST`; `plugins on PLUGIN` | Explicit current inspection digest; `--yes` grants nothing |
| `plugin-toggle:ID` | `plugins on ID` / `plugins off ID` | Stale approval requires review and explicit reapproval |
| `launch-device:cpu.min`, `cpu.max`, `gpu.min`, `gpu.max` | `launch-settings set KEY=PERCENT` | Same Device/Any place |
| `launch-device:cpu.governor` | `launch-settings set cpu.governor=VALUE` | Device/Any only, when advertised |
| `launch:set:PLACE#cpu.min`, `cpu.max`, `gpu.min`, `gpu.max` | `launch-settings set KEY=PERCENT [layer] [scope]` | Same place; no inferred game/release |
| `launch:set:PLACE#runner` | `launch-settings runner ID [layer]` | Any-runner scope; `--inherit` clears |
| `launch:set:PLACE#setting:KEY` | `launch-settings set KEY=VALUE [layer] [scope]` | Runner/family schemas define keys and types |
| `launch:set:PLACE#add-setting` | `launch-settings set KEY=VALUE [layer] [scope]` | Portal's Add alone changes no stored value |
| `launch:set:PLACE#modifier:ID:enabled` | `launch-settings set ID.enabled=true\|false [layer] [scope]` | Same modifier path |
| `launch:set:PLACE#modifier:ID:setting:KEY` | `launch-settings set ID.KEY=VALUE [layer] [scope]` | Same modifier schema |
| `launch:set:PLACE#modifier:ID:add-setting` | `launch-settings set ID.KEY=VALUE [layer] [scope]` | Add alone changes no stored value |
| Inherit / `launch:set:PLACE#clear:PATH` | `launch-settings set KEY --inherit [layer] [scope]` | Clears override; empty strings stay values |
| `launch:profile:pick` | `launch-settings profiles use ID --game GAME`; `use --none --game GAME` | Exact profile ID, including no-profile choice |
| `launch:profile:new` | `launch-settings profiles new TITLE --game GAME` | Same profile creation |
| `launch:profile:rename:ID` | **Gap: needs `launch-settings profiles rename` (added by the producers branch)** | Existing `LaunchProfileSave` supports `profileId`; excluded `launch_settings.rs` owns the fix |
| `launch:profile:remove:ID` | `launch-settings profiles delete ID --game GAME --yes` | Same revision and exact ID |
| `launch:reload`, `launch:retry`, `launch-device:retry` | `launch-settings show [--game GAME]`; explicitly repeat a failed command | No automatic conflict retry |
| `launch:runner` | `play GAME --runner ID`; `launch-settings runner ID --game GAME` | One-launch choice vs persisted choice |
| `launch:cancel` | Exit / do not submit a command | Navigation only |
| Pico `pico:font` (`surfaces/pico/src/pico-settings-view.ts`) | **Needs shared RPC; schema unresolved** | `pico-font-preference.ts` keeps `pico.font` in browser storage; the host treaty never receives it |
| Zone `zone:system-look` (`surfaces/zone/src/plan/options.ts`) | **Needs shared RPC; schema unresolved** | `zone-look-preference.ts` keeps `zone.look` in browser storage; the host treaty never receives it |
| `launch:set:PLACE#file:*`, `runner-facts:*`, unavailable GPU row | `launch-settings show [layer] [scope]` | Read-only facts; hooks, environment, arguments, working directory and raw config remain file-authored exceptions |

Layer flags: Device by default, `--system SYSTEM`, `--game GAME`,
`--release RELEASE --game GAME`, `--profile PROFILE --game GAME`.
Scope flags: Any by default, `--runner RUNNER`, `--family FAMILY`.
On this audit base System inspection still needs explicit `--game` context;
the producers branch owns the independent System snapshot change. No CLI picks
a representative game or release.

## Actions outside settings

| Portal producer / action | CLI command | Status / limits |
| --- | --- | --- |
| `SurfaceRoot.tsx`, `use-launchables.ts`: play a library title | `play GAME` | Exact ID wins; several titles give explicit terminal picker, piped/JSON lists and stops |
| `runner-chooser.ts`: inspect / pick / remember runner | `launch-settings show --game GAME`; `play GAME --runner ID`; `launch-settings runner ID --game GAME` | Same routes and persisted choice |
| `use-launchables.ts`: resume / end exact live session | `resume`; `end --yes` | Controls only the observed launch ID |
| `surface-model.ts`, `use-launchables.ts`: cancel startup / pending launch | **Gap in excluded `play.rs`** | Existing `SessionCancel` RPC; CLI has no cancel command. `end` is not an alias for reservation cancellation |
| `SurfaceRoot.tsx`: choose among launch locations | `--device NAME_OR_KEY play GAME` for an owner device | Remote worker owns signed peer session authorization; device selection is not a streaming-route flag |
| `shared-play.ts`: view invitations / Play on stream / Dismiss invitation | **Gap in excluded session area (`play.rs`)** | Existing `InvitesList`, `InvitesStreamPlay`, `InvitesDismiss` RPCs; no CLI verbs |
| `shared-play.ts`: leave stream / cancel stream connection | `leave` after viewer exists; **gap for in-flight connection cancellation** | Portal waits for `InvitesStreamPlay` then leaves its exact viewer; it never ends the far game |
| `overlay/live-overlay-controller.ts`: Resume / End | `resume`; `end --yes` | Same exact-session operations |
| `overlay/live-overlay-controller.ts`: plugin actions, toggles, choices, ranges | **Gap in excluded `play.rs`** | Existing `SessionControls`, `SessionControlInvoke` RPCs; no CLI commands |
| `overlay/live-overlay-controller.ts`: Open Korri / dismiss overlay | No stored change | Local presentation/navigation, not a daemon setting |
| `game-downloads.ts`: Download / Cancel / Remove download | `get GAME`; `downloads cancel --artifact ID`; `downloads remove GAME --yes` | Artifact IDs, not provider job IDs; saves stay |
| `acquisition-gate.ts`: download required artifacts before play / cancel gate | `downloads status --game GAME --runner ID`; `downloads start --artifact ID` (repeat flag); `downloads cancel --artifact ID`; then `play GAME` | **Gap closed:** explicit runner inspection and exact artifact start use the existing `AcquisitionStatus` / `AcquisitionStart` RPCs. No automatic downloads or guessed route |
| `store-acquisition.ts`: search / inspect / acquire provider claim | `get search QUERY --provider ID`; `get QUERY --provider ID --id CLAIM` | CLI gets details before acquisition; no separate details-only command |
| `store-acquisition.ts`: provider job progress / retry / cancel | `downloads status --job ID`; repeat `get`; `downloads cancel --job ID` | Existing provider job IDs; no job-list producer |
| `save-sync.ts`: inspect history / choose split / restore | `saves status`; `saves history GAME`; `saves choose GAME SNAPSHOT --yes`; `saves restore GAME SNAPSHOT --yes` | Local; keeps other history; restores explicitly overwrite |
| `network-onboarding.ts`: scan / connect saved or open Wi-Fi | `wifi scan`; `wifi connect ID` | Exact network ID |
| `network-onboarding.ts`: connect hidden Wi-Fi / supply password | `wifi connect SSID --hidden --password-stdin` | Same existing network schema; private input |
| `network-onboarding.ts`: finish / restart setup | `setup finish`; `setup restart` | Same onboarding marker; no new identity |
| Setup surfaces: configure device name and seats | `setup --name NAME --seats COUNT`; `devices rename NAME`; `controllers seats COUNT` | Same settings revisions |
| Setup surfaces: physical controller testing | **Recorded portal-only exception** | Requires physical semantic input; configured seat count is not a test |
| `starter-pack-onboarding.ts`: accept / skip optional starter pack | `plugins on @korri:starter-pack`; `library folders rescan`; or omit both | Existing effects; no separate starter-pack schema |
| `use-launchables.ts`: export active / retired identity backup | `identity backup [--retired PUBLIC_KEY]` | Password-encrypted output; private password input |
| `use-launchables.ts`: switch identity from encrypted backup / NIP-46 | `identity switch local`; `identity switch nip46` | Same coordinator and data disposition; trust-loss confirmation stays explicit |
| `use-launchables.ts`: delete retained identity key | `identity retired delete PUBLIC_KEY` | Same backup confirmation; `--yes` does not manufacture it |
| `surface-preference.ts`: `?surface=ID` chooses and remembers a surface | **Needs a shared RPC; schema unresolved** | Existing producer is browser-local `korri.surface` storage, not korrid. Do not edit Chromium files from CLI |
| Browse, search/filter a library, close sheets, retry reads, dismiss notices | `library games list`, `status`, or repeat the relevant read | Navigation/transient presentation; no settings mutation |

Sleep, power off and Restart Korri have no portal action/RPC on the audit base.
ADR 0031 defers these until a shared korrid producer exists; the producers
branch owns that work. They are not silently mapped to host shell commands.
Library folder add and attention assign/ignore are additional CLI paths;
this portal has no corresponding action site. Folder change exists in the
portal client interface but has no portal action site or CLI command. An
interface method alone does not establish an editable portal action.

## Guard and remaining work

`nix run .#cli-check -- FILTER` always runs `tests/parity.py` before Rust format,
lint and the focused Rust tests. The guard reads the actual portal setting rows,
plugin ID constructors, launch ID constructors, limit keys, dynamic editor
paths and the two surface-owned preference IDs. An unrecognized ID/path fails
and needs a command mapping in that change.
Its small test-local table is not a second runtime settings manifest. Negative
tests prove new literal, constant and dynamic IDs cannot pass without review.
The explicit profile-rename known gap must become a real mapping when the
producers command arrives; the check refuses a stale known-gap entry.

This guard checks the current settings producer seams, not arbitrary new portal
files or every action handler. Keep this action audit current during review.
A new producer must be added to the guard. Command mappings do not replace
real-process tests of effects and authorization.

The artifact-start gap uses `AcquisitionStartRequest.artifact_ids` from
`services/korrid/src/acquisition/types.rs`; runner-specific status uses that
file's existing `AcquisitionStatusRequest.runner_id`, as the portal acquisition
gate already does. No new persisted schema, RPC, field, setting key or device
operation was added by this audit. Existing-RPC gaps in reserved files are
reported rather than worked around with duplicate modules. Three browser-local
preference gaps need shared producer decisions: surface selection, Pico's font
and Zone's look. Their device-wide storage and authority contracts are
unresolved; no new exception to ADR 0031 is assumed. Read-only information gaps
(address/free storage) and power calls are separate producer work, not hidden
parity claims.
