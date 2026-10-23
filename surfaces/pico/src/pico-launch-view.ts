import type { SurfaceAction, SurfaceModel } from "@contracts/surface/korri-surface"
import { picoSettingRowFrom, type PicoSettingRowView, type PicoSettingsView } from "./pico-settings-view"

/**
 * A game's launch settings as Pico draws them: one spine per layer (Device,
 * a System, the Game, a Release, each Profile), and inside its card one tab
 * per runner scope (any runner, a family, each runner). A place is one layer
 * for one scope; its rows are one list, each of Korri's groups opened by its
 * caption (RUNNER, CPU, FROM FILES), because a place mixes rows the player
 * can change with rows only a file sets.
 *
 * Korri sends a layer's places in a row, so a layer is the run of places
 * that share `layer.id`.
 */
export interface PicoLaunchScopeView {
  readonly placeId: string
  readonly label: string
  readonly kind: "any" | "family" | "runner"
  readonly rows: readonly PicoSettingRowView[]
}

export interface PicoLaunchLayerView {
  readonly id: string
  readonly title: string
  readonly scopes: readonly PicoLaunchScopeView[]
}

export interface PicoLaunchView {
  readonly gameTitle?: string
  readonly phase: "Loading" | "Ready" | "Conflict" | "Error"
  /** Korri's words for a conflict or a failure. */
  readonly message?: string
  /** Korri's actions, Close last. Pico shows them on the notice that needs them. */
  readonly actions: readonly SurfaceAction[]
  /** Plain lines about the whole editor: places Korri cannot show yet, a
   * runner that is not installed, a runner whose facts are unavailable. */
  readonly notes: readonly string[]
  /** The action that closes the editor, for Back. */
  readonly closeActionId?: string
  readonly layers: readonly PicoLaunchLayerView[]
  /** Every row of every place, a group per layer: what the editor and its
   * questions look a row up in, whichever scope shows. */
  readonly settings: PicoSettingsView
}

export const picoLaunchOpen = (model: SurfaceModel): boolean =>
  model.launchSettings !== undefined && model.launchSettings._tag !== "Closed"

export function picoLaunchViewFromModel(model: SurfaceModel): PicoLaunchView | undefined {
  const launch = model.launchSettings
  if (launch === undefined || launch._tag === "Closed") return undefined
  const close = launch.actions.find(action => action.id.endsWith(":cancel"))
  const layers: { id: string; title: string; scopes: PicoLaunchScopeView[] }[] = []
  for (const place of launch.places) {
    const rows = place.groups.flatMap(group => group.items.map((item, index) => {
      const row = picoSettingRowFrom(item, model)
      return index === 0 ? { ...row, section: group.title.toUpperCase() } : row
    }))
    const last = layers.at(-1)
    const scope = { placeId: place.id, label: place.scope.label, kind: place.scope.kind, rows }
    if (last !== undefined && last.id === place.layer.id) last.scopes.push(scope)
    else layers.push({ id: place.layer.id, title: place.layer.title, scopes: [scope] })
  }
  return {
    ...(launch.gameTitle === undefined ? {} : { gameTitle: launch.gameTitle }),
    phase: launch._tag,
    ...(launch._tag === "Conflict" || launch._tag === "Error" ? { message: launch.message } : {}),
    notes: launch.notes,
    actions: launch.actions,
    ...(close === undefined ? {} : { closeActionId: close.id }),
    layers,
    settings: {
      groups: layers.map(layer => ({ title: layer.title.toUpperCase(), rows: layer.scopes.flatMap(scope => scope.rows) })),
    },
  }
}

/** The scope a layer shows: the one asked for, or its last when it has fewer. */
export const picoLaunchScope = (layer: PicoLaunchLayerView | undefined, scope: number): number =>
  layer === undefined ? 0 : Math.max(0, Math.min(scope, layer.scopes.length - 1))

/**
 * What the settings panel draws: a spine per layer, each holding the rows
 * of the scope showing. Only the layer showing matters to the panel; every
 * spine still names its layer.
 */
export function picoLaunchShown(view: PicoLaunchView, scope: number): PicoSettingsView {
  return {
    groups: view.layers.map(layer => ({
      title: layer.title.toUpperCase(),
      rows: layer.scopes[picoLaunchScope(layer, scope)]?.rows ?? [],
    })),
  }
}
