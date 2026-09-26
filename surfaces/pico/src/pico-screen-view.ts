import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { type PicoHomeView, picoHomeViewFromCatalog } from "./pico-home-view"

/**
 * The cartridge a launch is about: enough of the catalog entry to draw it.
 * Present only when Korri names the game and the catalog holds it.
 */
export interface PicoLaunchCart {
  readonly id: string
  readonly title: string
  readonly artUrl?: string
}

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
  | {
      readonly _tag: "Busy"
      readonly kicker: string
      readonly detail?: string
      readonly cart?: PicoLaunchCart
    }
  | {
      readonly _tag: "Running"
      readonly kicker: string
      readonly gameTitle?: string
      readonly cart?: PicoLaunchCart
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
 * Status outranks the catalog: while Korri is starting, running, or failing to
 * start a game, that is the truth about this device, whatever the library says.
 */
export function picoScreenViewFromModel(model: SurfaceModel): PicoScreenView {
  const status = model.status
  switch (status._tag) {
    case "Busy": {
      const cart = cartFor(model, status.gameId)
      return {
        _tag: "Busy",
        kicker: status.kicker,
        ...(status.detail === undefined ? {} : { detail: status.detail }),
        ...(cart === undefined ? {} : { cart }),
      }
    }
    case "Running": {
      const cart = cartFor(model, status.gameId)
      return {
        _tag: "Running",
        kicker: status.kicker,
        ...(cart === undefined ? {} : { gameTitle: cart.title, cart }),
      }
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

/** The catalog's own entry for Korri's id, or nothing: never a guess. */
function cartFor(
  model: SurfaceModel,
  gameId: string | undefined,
): PicoLaunchCart | undefined {
  if (gameId === undefined) return undefined
  if (model.catalog._tag !== "Ready") return undefined
  const game = model.catalog.games.find((candidate) => candidate.id === gameId)
  if (game === undefined) return undefined
  return {
    id: game.id,
    title: game.title,
    ...(game.coverArtUrl === undefined ? {} : { artUrl: game.coverArtUrl }),
  }
}
