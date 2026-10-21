import type { SurfaceAction, SurfaceModel } from "@contracts/surface/korri-surface"
import { picoSettingRowFrom, type PicoSettingsView } from "./pico-settings-view"

/**
 * A game's launch settings as Pico draws them: one spine per layer (Device,
 * System, Runner, Game, Release, each Profile), its rows in one list.
 *
 * Korri groups a layer's rows (CPU, GPU, Profile); their labels already say
 * which is which ("CPU min", "New profile"), so Pico lists them in Korri's
 * order without a second heading inside the card.
 */
export interface PicoLaunchView {
  readonly gameTitle?: string
  readonly phase: "Loading" | "Ready" | "Conflict" | "Error"
  /** Korri's words for a conflict or a failure. */
  readonly message?: string
  /** Korri's actions, Close last. Pico shows them on the notice that needs them. */
  readonly actions: readonly SurfaceAction[]
  /** One plain line about the whole editor: layers Korri cannot show yet. */
  readonly note?: string
  /** The action that closes the editor, for Back. */
  readonly closeActionId?: string
  readonly settings: PicoSettingsView
}

export const picoLaunchOpen = (model: SurfaceModel): boolean =>
  model.launchSettings !== undefined && model.launchSettings._tag !== "Closed"

export function picoLaunchViewFromModel(model: SurfaceModel): PicoLaunchView | undefined {
  const launch = model.launchSettings
  if (launch === undefined || launch._tag === "Closed") return undefined
  const close = launch.actions.find(action => action.id.endsWith(":cancel"))
  return {
    ...(launch.gameTitle === undefined ? {} : { gameTitle: launch.gameTitle }),
    phase: launch._tag,
    ...(launch._tag === "Conflict" || launch._tag === "Error" ? { message: launch.message } : {}),
    ...(launch.note === undefined ? {} : { note: launch.note }),
    actions: launch.actions,
    ...(close === undefined ? {} : { closeActionId: close.id }),
    settings: {
      groups: launch.layers.map(layer => ({
        title: layer.title.toUpperCase(),
        rows: layer.groups.flatMap(group => group.items.map(item => picoSettingRowFrom(item, model))),
      })),
    },
  }
}
