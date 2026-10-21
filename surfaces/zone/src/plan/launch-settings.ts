/**
 * What the LAUNCH SETTINGS page shows for Korri's launch settings editor: one
 * group per layer (Device, System, Runner, Game, Release, each Profile), in
 * Korri's order, every row of the layer under its title. The page reuses
 * OPTIONS' rows, keyboard and dialog, so it hands OPTIONS' plan the layers as
 * settings groups; the state model (src/state/surface.ts) runs OPTIONS'
 * update on the same slices, and the two can never disagree about the rows.
 *
 * Korri's own groups inside a layer (CPU, GPU, Profile) are not drawn: the
 * labels already say which is which ("CPU min", "New profile").
 */
import type { SurfaceAction, SurfaceLaunchSettings, SurfaceModel } from "@contracts/surface/korri-surface"
import type { ZoneOptionsSlices } from "./options"

export type ZoneLaunchOpen = Exclude<SurfaceLaunchSettings, { readonly _tag: "Closed" }>
export type ZoneLaunchState = ZoneLaunchOpen["_tag"]

export const launchOpen = (korri: Pick<SurfaceModel, "launchSettings">): boolean =>
  korri.launchSettings !== undefined && korri.launchSettings._tag !== "Closed"

export interface ZoneLaunchPlan {
  /** OPTIONS' slices: the layers as settings groups, and nothing of Zone's own. */
  readonly slices: ZoneOptionsSlices
  /** The rows show only while Korri is ready; otherwise its message does. */
  readonly rows: boolean
  /** Korri's actions, Close among them. */
  readonly actions: readonly SurfaceAction[]
  /** The action that closes the editor, for B. */
  readonly closeActionId?: string
}

export function launchPlan(
  launch: ZoneLaunchOpen,
  status: SurfaceModel["settingsStatus"],
): ZoneLaunchPlan {
  const close = launch.actions.find(action => action.id.endsWith(":cancel"))
  const rows = launch._tag === "Ready"
  return {
    slices: {
      settings: rows
        ? launch.layers.map(layer => ({ title: layer.title, items: layer.groups.flatMap(group => group.items) }))
        : [],
      settingsStatus: status,
      actions: [],
    },
    rows,
    actions: launch.actions,
    ...(close === undefined ? {} : { closeActionId: close.id }),
  }
}

/** The slices for OPTIONS' update while the editor is open; none while it is not. */
export function launchSlices(korri: Pick<SurfaceModel, "launchSettings" | "settingsStatus">): ZoneOptionsSlices | undefined {
  const launch = korri.launchSettings
  return launch === undefined || launch._tag === "Closed" ? undefined : launchPlan(launch, korri.settingsStatus).slices
}
