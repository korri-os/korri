/**
 * What the LAUNCH SETTINGS page shows for Korri's launch settings editor.
 *
 * Korri publishes places: a layer (Device, a System, the Game, a Release,
 * each Profile) for a runner scope (any runner, a family, one runner). Zone
 * shows one runner scope at a time, chosen on a RUNNER SCOPE row at the top that
 * steps like any OPTIONS choice (ZONE_LAUNCH_SCOPE_ROW, Zone's own, never
 * sent to Korri), and under it one group per layer, in Korri's order, every row of that
 * layer's place for the scope under the layer's title. A layer with no
 * place for the scope is left out. The page reuses OPTIONS' rows, keyboard
 * and dialog, so it hands OPTIONS' plan the layers as settings groups; the
 * state model (src/state/surface.ts) runs OPTIONS' update on the same
 * slices, and the two can never disagree about the rows.
 *
 * Korri's own groups inside a place (RUNNER, CPU, FROM FILES) are not drawn:
 * the labels already say which is which ("CPU min", "Environment PATH"),
 * and a row only a file sets says so under it.
 */
import type { SurfaceAction, SurfaceLaunchSettings, SurfaceModel } from "@contracts/surface/korri-surface"
import type { ZoneOptionsSlices } from "./options"

export type ZoneLaunchOpen = Exclude<SurfaceLaunchSettings, { readonly _tag: "Closed" }>
export type ZoneLaunchState = ZoneLaunchOpen["_tag"]

export const launchOpen = (korri: Pick<SurfaceModel, "launchSettings">): boolean =>
  korri.launchSettings !== undefined && korri.launchSettings._tag !== "Closed"

/** One runner scope, as the strip names it. */
export interface ZoneLaunchScope {
  readonly kind: "any" | "family" | "runner"
  readonly label: string
}

export interface ZoneLaunchPlan {
  /** OPTIONS' slices: the layers as settings groups, and nothing of Zone's own. */
  readonly slices: ZoneOptionsSlices
  /** The rows show only while Korri is ready; otherwise its message does. */
  readonly rows: boolean
  /** Every runner scope Korri publishes a place for, in Korri's order. */
  readonly scopes: readonly ZoneLaunchScope[]
  /** The scope shown, held inside `scopes`. */
  readonly scope: number
  /** Korri's actions, Close among them. */
  readonly actions: readonly SurfaceAction[]
  /** The action that closes the editor, for B. */
  readonly closeActionId?: string
}

/** Zone's own RUNNER SCOPE row: which runner scope shows. Never sent to Korri. */
export const ZONE_LAUNCH_SCOPE_ROW = "zone:launch-scope"

/** The scope a step on the RUNNER SCOPE row lands on, round the ends. */
export const launchScopeStep = (count: number, scope: number, direction: -1 | 1): number =>
  count === 0 ? 0 : (scope + direction + count) % count

const sameScope = (a: ZoneLaunchScope, b: ZoneLaunchScope) => a.kind === b.kind && a.label === b.label

/** The scopes in the order Korri first publishes them. */
export function launchScopes(launch: ZoneLaunchOpen): readonly ZoneLaunchScope[] {
  const scopes: ZoneLaunchScope[] = []
  for (const place of launch.places) {
    if (!scopes.some(scope => sameScope(scope, place.scope))) scopes.push({ kind: place.scope.kind, label: place.scope.label })
  }
  return scopes
}

export function launchPlan(
  launch: ZoneLaunchOpen,
  status: SurfaceModel["settingsStatus"],
  scope = 0,
): ZoneLaunchPlan {
  const close = launch.actions.find(action => action.id.endsWith(":cancel"))
  const rows = launch._tag === "Ready"
  const scopes = launchScopes(launch)
  const shown = Math.max(0, Math.min(scope, scopes.length - 1))
  const chosen = scopes[shown]
  const places = chosen === undefined ? [] : launch.places.filter(place => sameScope(place.scope, chosen))
  const runner = {
    title: "Runner",
    items: [{
      id: ZONE_LAUNCH_SCOPE_ROW,
      label: "Runner scope",
      value: chosen?.label ?? "",
      description: "Which runners the values below apply to.",
      interaction: {
        kind: "choice" as const,
        choices: scopes.map((candidate, index) => ({ value: String(index), label: candidate.label })),
        selected: String(shown),
      },
    }],
  }
  return {
    slices: {
      settings: rows
        ? [
            ...(scopes.length > 1 ? [runner] : []),
            ...places.map(place => ({ title: place.layer.title, items: place.groups.flatMap(group => group.items) })),
          ]
        : [],
      settingsStatus: status,
      actions: [],
    },
    rows,
    scopes,
    scope: shown,
    actions: launch.actions,
    ...(close === undefined ? {} : { closeActionId: close.id }),
  }
}

/** The slices for OPTIONS' update while the editor is open; none while it is not. */
export function launchSlices(korri: Pick<SurfaceModel, "launchSettings" | "settingsStatus">, scope = 0): ZoneOptionsSlices | undefined {
  const launch = korri.launchSettings
  return launch === undefined || launch._tag === "Closed" ? undefined : launchPlan(launch, korri.settingsStatus, scope).slices
}
