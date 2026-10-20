/**
 * What the RUN WITH page shows, in order, for Korri's runner choice: the
 * content's warnings, its message, one row per route with the route's
 * warnings and actions under it, then the content's own actions.
 *
 * Shared by the page (which draws it) and src/state/run-with.ts (which keeps
 * the cursor on it), so the two name the same rows the same way.
 */
import type { SurfaceAction, SurfaceRunnerChoice, SurfaceRunnerRoute } from "@contracts/surface/korri-surface"

export type ZoneRunWithOpen = Exclude<SurfaceRunnerChoice, { readonly _tag: "Closed" }>
export type ZoneRunWithState = ZoneRunWithOpen["_tag"]

export const zoneRouteRowId = (runnerId: string) => `route:${runnerId}`
export const zoneSavedRowId = (label: string) => `saved:${label}`

/** A route and how the saved choices name it. */
export interface ZoneRunWithRoute {
  readonly id: string
  readonly route: SurfaceRunnerRoute
  /** The saved choices that pick this route: "This game", "System gba". */
  readonly saved: readonly string[]
}

export interface ZoneRunWithPlan {
  readonly routes: readonly ZoneRunWithRoute[]
  /** Saved choices whose runner no route offers (a Stale choice). */
  readonly orphans: readonly { readonly label: string; readonly runnerId: string }[]
  /** Every focusable id, top to bottom: a route's row id, or an action's own id. */
  readonly ids: readonly string[]
  /** Every action on the page by its id, to run it. */
  readonly actions: ReadonlyMap<string, SurfaceAction>
}

export function runWithPlan(choice: ZoneRunWithOpen): ZoneRunWithPlan {
  const routes = choice.routes.map(route => ({
    id: zoneRouteRowId(route.runnerId),
    route,
    saved: choice.saved.filter(saved => saved.runnerId === route.runnerId).map(saved => saved.label),
  }))
  const orphans = choice.saved.filter(saved => !choice.routes.some(route => route.runnerId === saved.runnerId))
  const all = [...choice.routes.flatMap(route => route.actions), ...choice.actions]
  const ids = [
    ...orphans.map(orphan => zoneSavedRowId(orphan.label)),
    ...routes.flatMap(entry => [entry.id, ...entry.route.actions.map(action => action.id)]),
    ...choice.actions.map(action => action.id),
  ]
  return { routes, orphans, ids, actions: new Map(all.map(action => [action.id, action])) }
}
