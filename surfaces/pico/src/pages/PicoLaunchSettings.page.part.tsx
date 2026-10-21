import type { SurfaceLaunchSettings, SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import { launchLayerIndex, launchSettingsFixtures as launch, withLaunchSettings } from "../fixtures/launch-settings"
import { performPicoRequest } from "../pico-host"
import { picoLaunchViewFromModel } from "../pico-launch-view"
import type { PicoMessage } from "../state/messages"
import { initialNavigation, type PicoNavigation } from "../state/navigation"
import { update } from "../state/update"
import { usePicoProgram } from "../use-pico-program"
import { PicoLaunchSettings } from "./PicoLaunchSettings"

export const name = "Launch Settings"
export const note = "A game's launch settings: a spine per layer, each value's source, device-only, applies at next launch"

/* The surface's navigation program against the fixture host, as on the
 * device: the spine, the keyboard and the delete question are Pico's. */
function Launch({
  model,
  opening = [],
}: {
  readonly model: SurfaceModel
  readonly opening?: readonly PicoMessage[]
}) {
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
      onChange={(settingId, value) => dispatch({ _tag: "ChangedSetting", settingId, value })}
      onCloseEditor={() => dispatch({ _tag: "ClosedEditor" })}
      onConfirm={() => dispatch({ _tag: "ConfirmedSettingAction" })}
      onConfirmClear={() => dispatch({ _tag: "ConfirmedClear" })}
      onDismissProblem={() => dispatch({ _tag: "DismissedSettingsProblem" })}
      onEdit={settingId => dispatch({ _tag: "OpenedEditor", settingId })}
      onGroup={group => dispatch({ _tag: "ChoseSettingsGroup", group })}
      onRun={actionId => dispatch({ _tag: "PressedSettingAction", actionId })}
      onSaveText={() => dispatch({ _tag: "SavedText" })}
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

const on = (editor: SurfaceLaunchSettings, layerId: string): PicoMessage =>
  ({ _tag: "ChoseSettingsGroup", group: launchLayerIndex(editor, layerId) })
const open = (editor: SurfaceLaunchSettings, savingId?: string) => withLaunchSettings(fixtureModel, editor, savingId)

export function Loading() { return <Launch model={open(launch.loading)} /> }
/** The Runner layer sets the CPU floor itself: an overridden row. */
export function ReadyOverridden() {
  return <Launch model={open(launch.ready)} opening={[on(launch.ready, "runner:retroarch/mgba")]} />
}
/** The Game layer inherits its CPU values: each row names where they come from. */
export function ReadyInherited() {
  return <Launch model={open(launch.ready)} opening={[on(launch.ready, "game")]} />
}
/** The Device layer, with the governor only it carries. */
export function ReadyDevice() { return <Launch model={open(launch.ready)} /> }
/** The selected profile's own layer: its values, its name and Delete. */
export function ProfileLayer() {
  return <Launch model={open(launch.ready)} opening={[on(launch.ready, "profile:battery")]} />
}
export function ProfileNone() {
  return <Launch model={open(launch.noProfiles)} opening={[on(launch.noProfiles, "game")]} />
}
export function ProfilesMany() {
  return <Launch model={open(launch.manyProfiles)} opening={[on(launch.manyProfiles, "profile:word")]} />
}
export function NewProfile() {
  return (
    <Launch
      model={open(launch.ready)}
      opening={[on(launch.ready, "game"), { _tag: "OpenedEditor", settingId: "launch:profile:new" },
        ...[..."Race"].map(character => ({ _tag: "TypedText", character }) as PicoMessage)]}
    />
  )
}
export function ConfirmDeleteProfile() {
  const editor = launch.ready
  const row = editor.layers.find(layer => layer.id === "profile:battery")?.groups.flatMap(group => group.items)
    .find(item => item.label === "Delete profile")
  const interaction = row?.interaction
  return (
    <Launch
      model={open(editor)}
      opening={[on(editor, "profile:battery"), ...(interaction?.kind === "action" && interaction.confirmation !== undefined
        ? [{ _tag: "AskedSettingConfirmation", actionId: interaction.actionId, confirmation: { ...interaction.confirmation, destructive: true } } as PicoMessage]
        : [])]}
    />
  )
}
export function Saving() {
  return <Launch model={open(launch.ready, "launch:set:game:gpu.max")} opening={[on(launch.ready, "game")]} />
}
export function Conflict() { return <Launch model={open(launch.conflict)} /> }
export function Error() { return <Launch model={open(launch.error)} /> }
/** No runner chosen: Device, Game and the profile stay editable; one line says what is missing. */
export function NeedsRunnerChoice() {
  return <Launch model={open(launch.needsRunner)} opening={[on(launch.needsRunner, "game")]} />
}
export function RouteUnavailable() {
  return <Launch model={open(launch.routeUnavailable)} opening={[on(launch.routeUnavailable, "game")]} />
}
/** No device facts: 10 % steps, and the rows say so. */
export function FactsMissing() {
  return <Launch model={open(launch.factsMissing)} opening={[on(launch.factsMissing, "game")]} />
}

export default function PicoLaunchSettingsPagePart() {
  return <Launch model={open(launch.ready)} />
}
