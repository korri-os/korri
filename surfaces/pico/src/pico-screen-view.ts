import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { type PicoHomeView, picoHomeViewFromCatalog } from "./pico-home-view"

/**
 * Everything the screen can be showing, as one closed set.
 *
 * The catalog cases and the launch cases are deliberately one union rather than
 * two overlapping ones: at any moment the screen shows exactly one thing, and
 * modelling that as "a catalog state plus a launch state" would let a caller
 * render both and leave the precedence rule implicit in whichever branch it
 * wrote first.
 */
export type PicoScreenView =
  | PicoHomeView
  | { readonly _tag: "Busy"; readonly kicker: string; readonly detail?: string }
  | {
      readonly _tag: "Running"
      readonly kicker: string
      readonly gameTitle?: string
    }
  | {
      readonly _tag: "Problem"
      readonly kicker: string
      readonly reason: string
      readonly canRetry: boolean
      readonly gameTitle?: string
    }

/**
 * What the screen shows, decided once.
 *
 * Status outranks the catalog: while Korri is starting a game, failing to start
 * one, or running one, that is the truth about this device, and a shelf drawn
 * over it would invite the user to launch something else on top. The shelf is
 * only the answer when nothing else is happening.
 *
 * A `Problem` names its own game through `gameTitle` rather than whatever the
 * shelf happens to have focused, because the failure belongs to the game Korri
 * says it belongs to.
 */
export function picoScreenViewFromModel(model: SurfaceModel): PicoScreenView {
  const status = model.status
  switch (status._tag) {
    case "Busy":
      return {
        _tag: "Busy",
        kicker: status.kicker,
        ...(status.detail === undefined ? {} : { detail: status.detail }),
      }
    case "Running":
      return {
        _tag: "Running",
        kicker: status.kicker,
        ...(gameTitleFor(model, status.gameId) === undefined
          ? {}
          : { gameTitle: gameTitleFor(model, status.gameId) }),
      }
    case "Problem":
      return {
        _tag: "Problem",
        kicker: status.kicker,
        reason: status.reason,
        canRetry: status.canRetry,
        ...(status.gameTitle === undefined
          ? {}
          : { gameTitle: status.gameTitle }),
      }
    case "Browsing":
      return picoHomeViewFromCatalog(model.catalog)
  }
}

/**
 * `Running` carries an id but no title, so the title has to come from the
 * catalog. Absent when Korri did not say which game is running, or when the
 * catalog does not hold it — never guessed from what the user last touched.
 */
function gameTitleFor(
  model: SurfaceModel,
  gameId: string | undefined,
): string | undefined {
  if (gameId === undefined) return undefined
  if (model.catalog._tag !== "Ready") return undefined
  return model.catalog.games.find((game) => game.id === gameId)?.title
}
