import type { SurfaceAction, SurfaceModel, SurfaceSettingItem } from "./model"
import { picoSettingRowFrom, type PicoSettingRowView, type PicoSettingsView } from "./pico-settings-view"

/**
 * A game's launch settings as Pico draws them (ADR 0066): one list. Each
 * setting stands once, in one card, the first row of each setting section
 * under the section's caption (VIDEO, AUDIO, STREAM), with the value the
 * next launch would use. The list is the same for a game that runs here and
 * a game that another device runs; `runsOn` says which, in Korri's words.
 *
 * A row the person changed and did not keep yet carries its earlier value
 * (`was`). While at least one row is changed Korri publishes the keep row:
 * where the changes are kept. Pico draws it under the list, always in view,
 * and a press on it opens Pico's list of the places.
 *
 * A row Korri gives no way to change (its device cannot be reached, or the
 * runner cannot honour the setting) is a fact: it takes the cursor, and its
 * description says why.
 */
export interface PicoLaunchView {
  readonly gameTitle?: string
  readonly phase: "Loading" | "Ready" | "Conflict" | "Error"
  /** Korri's words for a conflict or a failure. */
  readonly message?: string
  /** Where the game runs, in Korri's words: "Runs here", "Runs on aka". */
  readonly runsOn?: string
  /** Korri's actions, Close last. Pico shows them on the notice that needs them. */
  readonly actions: readonly SurfaceAction[]
  /** Plain lines about the whole editor, in Korri's words. */
  readonly notes: readonly string[]
  /** The action that closes the editor, for Back. */
  readonly closeActionId?: string
  /** The one list, top to bottom. Empty while Korri reads, and under a conflict. */
  readonly rows: readonly PicoSettingRowView[]
  /** Where the changes are kept. Present only while a row is changed. */
  readonly keep?: PicoSettingRowView
  /** Every row by its section, and the keep row: what the editor and its
   * questions look a row up in. */
  readonly settings: PicoSettingsView
}

export const picoLaunchOpen = (model: SurfaceModel): boolean =>
  model.launchSettings !== undefined && model.launchSettings._tag !== "Closed"

/**
 * The keep row: a list to choose from. Korri names the place in force, so
 * the row shows it; a press opens every place, the one in force first under
 * the cursor.
 */
function keepRowFrom(item: SurfaceSettingItem, model: SurfaceModel): PicoSettingRowView {
  const row = picoSettingRowFrom(item, model)
  const interaction = item.interaction
  if (interaction?.kind !== "choice") return row
  const chosen = interaction.choices.find(choice => choice.value === interaction.selected)
  return {
    ...row,
    ...(chosen === undefined ? {} : { value: chosen.label }),
    control: {
      kind: "pick",
      options: interaction.choices.map(({ value, label }) => ({ value, label })),
      ...(chosen === undefined ? {} : { selected: chosen.value }),
    },
  }
}

export function picoLaunchViewFromModel(model: SurfaceModel): PicoLaunchView | undefined {
  const launch = model.launchSettings
  if (launch === undefined || launch._tag === "Closed") return undefined
  const close = launch.actions.find(action => action.id.endsWith(":cancel"))
  // The rows show while Korri is ready, and beside an error: the rows of this device still work then.
  const shown = launch._tag === "Ready" || launch._tag === "Error"
  const was = new Map(shown ? (launch.changes ?? []).map(change => [change.id, change.was] as const) : [])
  const groups = (shown ? launch.groups ?? [] : []).filter(group => group.items.length > 0).map(group => ({
    title: group.title.toUpperCase(),
    rows: group.items.map((item, index): PicoSettingRowView => {
      // Korri's `source` says where the value stays; the device-only mark would say it twice.
      const { deviceOnly: _, ...row } = picoSettingRowFrom(item, model)
      const earlier = was.get(item.id)
      return {
        ...row,
        ...(index === 0 ? { section: group.title.toUpperCase() } : {}),
        ...(earlier === undefined ? {} : { was: earlier }),
      }
    }),
  }))
  const keep = shown && was.size > 0 && launch.keep !== undefined ? keepRowFrom(launch.keep, model) : undefined
  return {
    ...(launch.gameTitle === undefined ? {} : { gameTitle: launch.gameTitle }),
    phase: launch._tag,
    ...(launch._tag === "Conflict" || launch._tag === "Error" ? { message: launch.message } : {}),
    ...(launch.runsOn === undefined ? {} : { runsOn: launch.runsOn }),
    notes: launch.notes,
    actions: launch.actions,
    ...(close === undefined ? {} : { closeActionId: close.id }),
    rows: groups.flatMap(group => group.rows),
    ...(keep === undefined ? {} : { keep }),
    settings: { groups: [...groups, ...(keep === undefined ? [] : [{ title: "KEEP", rows: [keep] }])] },
  }
}
