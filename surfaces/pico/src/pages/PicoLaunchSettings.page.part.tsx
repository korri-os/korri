import type { SurfaceLaunchSettings, SurfaceModel } from "@contracts/surface/korri-surface"
import { useEffect } from "react"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import {
  launchLayerIndex,
  launchRowId,
  launchScopeIndex,
  launchSettingsFixtures as launch,
  withLaunchSettings,
} from "../fixtures/launch-settings"
import { performPicoRequest } from "../pico-host"
import { picoLaunchViewFromModel } from "../pico-launch-view"
import type { PicoMessage } from "../state/messages"
import { initialNavigation, type PicoNavigation } from "../state/navigation"
import { update } from "../state/update"
import { usePicoProgram } from "../use-pico-program"
import { PicoLaunchSettings } from "./PicoLaunchSettings"

export const name = "Launch Settings"
export const note = "A game's launch settings: a spine per layer, a tab per runner scope, each value's source and scope"

/* The surface's navigation program against the fixture host, as on the
 * device: the spine, the scope, the keyboard, the list and the delete
 * question are Pico's. */
function Launch({
  model,
  opening = [],
  cursor,
}: {
  readonly model: SurfaceModel
  readonly opening?: readonly PicoMessage[]
  /** The row the cursor starts on, by its label, as the d-pad would bring it. */
  readonly cursor?: string
}) {
  useEffect(() => {
    if (cursor === undefined) return
    const rows = document.querySelectorAll<HTMLElement>(".pico-setting-row .pico-row")
    ;[...rows].find(row => row.textContent?.startsWith(cursor) === true)?.focus()
  }, [cursor])
  const host = createFixtureHost()
  const { model: nav, dispatch } = usePicoProgram<PicoNavigation, PicoMessage, SurfaceModel>(
    () => [{ _tag: "KorriPublished" } as PicoMessage, ...opening].reduce((current, message) => update(current, message, model).model, initialNavigation()),
    update,
    model,
    request => performPicoRequest(host, { chooseFont: () => undefined }, request),
  )
  const view = picoLaunchViewFromModel(model)
  if (view === undefined) return null
  const question = nav.launch?.question ?? { _tag: "None" }
  const editing = question._tag === "EditingText" || question._tag === "ConfirmingClear"
    ? { settingId: question.settingId, clearing: question._tag === "ConfirmingClear", draft: question.draft }
    : undefined
  return (
    <PicoLaunchSettings
      asking={question._tag === "ConfirmingAction" ? question : undefined}
      clockLabel={model.clockLabel}
      editing={editing}
      group={nav.launch?.group ?? 0}
      launch={view}
      onAction={actionId => dispatch({ _tag: "PressedLaunchAction", actionId })}
      onAsk={(actionId, confirmation) => dispatch({ _tag: "AskedSettingConfirmation", actionId, confirmation })}
      onAskClear={() => dispatch({ _tag: "AskedClear" })}
      onCancel={() => dispatch({ _tag: "CancelledSettingAction" })}
      onCancelClear={() => dispatch({ _tag: "CancelledClear" })}
      onCancelPick={() => dispatch({ _tag: "CancelledPicker" })}
      onChange={(settingId, value) => dispatch({ _tag: "ChangedSetting", settingId, value })}
      onCloseEditor={() => dispatch({ _tag: "ClosedEditor" })}
      onConfirm={() => dispatch({ _tag: "ConfirmedSettingAction" })}
      onConfirmClear={() => dispatch({ _tag: "ConfirmedClear" })}
      onDismissProblem={() => dispatch({ _tag: "DismissedSettingsProblem" })}
      onEdit={settingId => dispatch({ _tag: "OpenedEditor", settingId })}
      onGroup={group => dispatch({ _tag: "ChoseSettingsGroup", group })}
      onPick={settingId => dispatch({ _tag: "OpenedPicker", settingId })}
      onPicked={value => dispatch({ _tag: "PickedChoice", value })}
      onRun={actionId => dispatch({ _tag: "PressedSettingAction", actionId })}
      onSaveText={() => dispatch({ _tag: "SavedText" })}
      onScope={scope => dispatch({ _tag: "ChoseLaunchScope", scope })}
      {...(question._tag === "PickingChoice" ? { picking: question.settingId } : {})}
      scope={nav.launch?.scope ?? 0}
      typing={{
        onType: character => dispatch({ _tag: "TypedText", character }),
        onBackspace: () => dispatch({ _tag: "PressedTextBackspace" }),
        onClear: () => dispatch({ _tag: "ClearedText" }),
        onToggleCapitals: () => dispatch({ _tag: "ToggledCapitals" }),
        onToggleSymbols: () => dispatch({ _tag: "ToggledSymbols" }),
      }}
    />
  )
}

/** Open on one layer, and on one runner scope in it. */
const on = (editor: SurfaceLaunchSettings, layerId: string, scope?: string): PicoMessage[] => [
  { _tag: "ChoseSettingsGroup", group: launchLayerIndex(editor, layerId) },
  ...(scope === undefined ? [] : [{ _tag: "ChoseLaunchScope", scope: launchScopeIndex(editor, layerId, scope) } as PicoMessage]),
]
const open = (editor: SurfaceLaunchSettings, savingId?: string) => withLaunchSettings(fixtureModel, editor, savingId)
const MGBA_GAME = "game:wario/runner:retroarch/mgba"

export function Loading() { return <Launch model={open(launch.loading)} /> }
/** The Device place for any runner: its runner choice, limits, governor and routine. */
export function ReadyDevice() { return <Launch model={open(launch.ready)} /> }
/** The Device place for mGBA sets the CPU floor itself: an overridden row. */
export function ReadyOverridden() {
  return <Launch model={open(launch.ready)} opening={on(launch.ready, "device", "mGBA")} />
}
/** The Game place inherits its CPU values; each row names where they come from. */
export function ReadyInherited() {
  return <Launch model={open(launch.ready)} opening={on(launch.ready, "game:wario")} />
}
/** mGBA's settings on the Game: a number, an enum, and Add setting. */
export function RunnerSettings() {
  return <Launch model={open(launch.ready)} opening={on(launch.ready, "game:wario", "mGBA")} />
}
/** gpSP is not the runner in use: its rows say so. */
export function NotInForce() {
  return <Launch model={open(launch.ready)} opening={on(launch.ready, "game:wario", "gpSP")} />
}
/** Add setting's list, open over the screen. */
export function AddSetting() {
  return (
    <Launch
      model={open(launch.ready)}
      opening={[...on(launch.ready, "game:wario", "mGBA"), { _tag: "OpenedPicker", settingId: launchRowId(MGBA_GAME, "add-setting") }]}
    />
  )
}
/** Frameskip in the keyboard, with its range and INHERIT. */
export function NumberEditor() {
  return (
    <Launch
      model={open(launch.ready)}
      opening={[...on(launch.ready, "game:wario", "mGBA"), { _tag: "OpenedEditor", settingId: launchRowId(MGBA_GAME, "setting:frameskip") }]}
    />
  )
}
/** The Game for the RetroArch family: a hook only a file sets, shown read-only. */
export function FileOnly() {
  return <Launch model={open(launch.ready)} opening={on(launch.ready, "game:wario", "Family retroarch")} />
}
/** The selected profile's own layer: its values, its name and Delete. */
export function ProfileLayer() {
  return <Launch model={open(launch.ready)} opening={on(launch.ready, "profile:battery")} />
}
export function ProfileNone() {
  return <Launch model={open(launch.noProfiles)} opening={on(launch.noProfiles, "game:wario")} />
}
export function ProfilesMany() {
  return <Launch model={open(launch.manyProfiles)} opening={on(launch.manyProfiles, "profile:word")} />
}
export function NewProfile() {
  return (
    <Launch
      model={open(launch.ready)}
      opening={[...on(launch.ready, "game:wario"), { _tag: "OpenedEditor", settingId: "launch:profile:new" },
        ...[..."Race"].map(character => ({ _tag: "TypedText", character }) as PicoMessage)]}
    />
  )
}
export function ConfirmDeleteProfile() {
  const editor = launch.ready
  const row = editor.places.find(place => place.id === "profile:battery/any")?.groups.flatMap(group => group.items)
    .find(item => item.label === "Delete profile")
  const interaction = row?.interaction
  return (
    <Launch
      model={open(editor)}
      opening={[...on(editor, "profile:battery"), ...(interaction?.kind === "action" && interaction.confirmation !== undefined
        ? [{ _tag: "AskedSettingConfirmation", actionId: interaction.actionId, confirmation: { ...interaction.confirmation, destructive: true } } as PicoMessage]
        : [])]}
    />
  )
}
export function Saving() {
  return <Launch model={open(launch.ready, launchRowId("game:wario/any", "gpu.max"))} opening={on(launch.ready, "game:wario")} />
}
export function Conflict() { return <Launch model={open(launch.conflict)} /> }
export function Error() { return <Launch model={open(launch.error)} /> }
/** No runner chosen: Device, Game and the profile stay editable; one line says what is missing. */
export function NeedsRunnerChoice() {
  return <Launch model={open(launch.needsRunner)} opening={on(launch.needsRunner, "game:wario")} />
}
export function RouteUnavailable() {
  return <Launch model={open(launch.routeUnavailable)} opening={on(launch.routeUnavailable, "game:wario")} />
}
/** The Game names a runner that isn't installed: a note, and the row says so. */
export function RunnerUnavailable() {
  return <Launch model={open(launch.runnerUnavailable)} opening={on(launch.runnerUnavailable, "game:wario")} />
}
/** gpSP's dry run failed: a note, and a read-only row in gpSP's places. */
export function RunnerFactsUnavailable() {
  return <Launch model={open(launch.factsUnavailable)} opening={on(launch.factsUnavailable, "game:wario", "gpSP")} />
}
/** Facts present, but no devfreq GPU: one read-only GPU line instead of choices. */
export function NoGpu() { return <Launch model={open(launch.noGpu)} opening={on(launch.noGpu, "game:wario")} /> }
/** Battery saver not picked: Gamescope inherits On from the Device, and says so. */
export function GamescopeInherited() {
  return <Launch cursor="Gamescope" model={open(launch.noProfilePicked)} opening={on(launch.noProfilePicked, "game:wario")} />
}
/** Battery saver turns Gamescope off for itself. */
export function GamescopeOffInProfile() {
  return <Launch cursor="Gamescope" model={open(launch.ready)} opening={on(launch.ready, "profile:battery")} />
}
/** MangoHud describes no settings: on, off or Inherit, and nothing to add. */
export function ModifierNoSchema() {
  return <Launch cursor="MangoHud" model={open(launch.ready)} opening={on(launch.ready, "game:wario")} />
}
/** No device facts: 10 % steps, and the rows say so. */
export function FactsMissing() {
  return <Launch model={open(launch.factsMissing)} opening={on(launch.factsMissing, "game:wario")} />
}

export default function PicoLaunchSettingsPagePart() {
  return <Launch model={open(launch.ready)} />
}
