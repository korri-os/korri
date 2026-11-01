import { picoDetailViewFromGame, picoDownloadLabel, picoStatsFor } from "./pico-detail-view"
import { picoHeroPick } from "./pico-library-view"
import type { PicoLaunchCart } from "./pico-screen-view"
import type { PicoShelfGame } from "./pico-shelf-game"
import type { PicoShown } from "./state/shown"

/** A game as the showing display draws it: the stage's own facts. */
export interface PicoShowcaseGame {
  readonly id: string
  readonly title: string
  readonly subtitle?: string
  readonly artUrl?: string
  readonly kicker?: string
  readonly stats: readonly { readonly figure: string; readonly caption: string }[]
  readonly resumable: boolean
  readonly download?: string
  /** Where the chosen cart stands on the shelf, when there is a shelf. */
  readonly position?: { readonly index: number; readonly total: number }
}

/**
 * What the display that shows draws, when Pico has one.
 *
 * `Game` is the cart the cursor chose, the hero's pick or the open game.
 * `Launch` is a game starting, running or paused, without its controls.
 * `Idle` is every screen with nothing to show: Settings, Find, the store,
 * setup, a library being read, empty or unreadable, and Korri's questions.
 * Their words and their buttons are on the other display.
 */
export type PicoShowcaseView =
  | { readonly _tag: "Game"; readonly game: PicoShowcaseGame }
  | {
      readonly _tag: "Launch"
      readonly phase: "starting" | "running" | "paused"
      readonly kicker: string
      readonly detail?: string
      readonly gameTitle?: string
      readonly cart?: PicoLaunchCart
      readonly progress?: { readonly done: number; readonly total?: number }
    }
  | { readonly _tag: "Idle" }

const IDLE: PicoShowcaseView = { _tag: "Idle" }

/** A cart from the shelf, as the stage draws it. */
function fromShelf(game: PicoShelfGame, kicker: string | undefined, position?: PicoShowcaseGame["position"]): PicoShowcaseView {
  const download = picoDownloadLabel(game)
  return {
    _tag: "Game",
    game: {
      id: game.id,
      title: game.title,
      ...(game.subtitle === undefined ? {} : { subtitle: game.subtitle }),
      ...(game.artUrl === undefined ? {} : { artUrl: game.artUrl }),
      ...(kicker === undefined ? {} : { kicker }),
      stats: picoStatsFor(game),
      resumable: game.resumable === true,
      ...(download === undefined ? {} : { download }),
      ...(position === undefined ? {} : { position }),
    },
  }
}

/**
 * The showing display's content, from what the catalog shows now.
 *
 * The shelf and the grid show the chosen cart, or the first when none is
 * chosen, as the shelf's own stage does. The hero shows its pick, with the
 * reason it leads.
 */
export function picoShowcaseFrom(shown: PicoShown): PicoShowcaseView {
  switch (shown._tag) {
    case "Detail": {
      const game = picoDetailViewFromGame(shown.game)
      return {
        _tag: "Game",
        game: {
          id: game.id,
          title: game.title,
          ...(game.subtitle === undefined ? {} : { subtitle: game.subtitle }),
          ...(game.artUrl === undefined ? {} : { artUrl: game.artUrl }),
          stats: game.stats,
          resumable: game.primaryLabel === "CONTINUE",
          ...(game.download === undefined ? {} : { download: game.download }),
        },
      }
    }
    case "Home":
      return fromHome(shown)
    case "Settings":
    case "Find":
    case "Store":
      return IDLE
  }
}

function fromHome(shown: Extract<PicoShown, { _tag: "Home" }>): PicoShowcaseView {
  const view = shown.view
  switch (view._tag) {
    case "Shelf": {
      if (shown.home.mode === "hero") {
        const pick = picoHeroPick(view.games)
        return pick === undefined ? IDLE : fromShelf(pick.game, pick.reason)
      }
      const found = view.games.findIndex(game => game.id === shown.home.selectedGameId)
      const index = found === -1 ? 0 : found
      const game = view.games[index]
      return game === undefined
        ? IDLE
        : fromShelf(game, game.section?.toUpperCase(), { index: index + 1, total: view.games.length })
    }
    case "Busy":
      return {
        _tag: "Launch",
        phase: "starting",
        kicker: view.kicker,
        ...(view.detail === undefined ? {} : { detail: view.detail }),
        ...(view.cart === undefined ? {} : { cart: view.cart }),
        ...(view.progress === undefined ? {} : { progress: view.progress }),
      }
    case "Running":
      return {
        _tag: "Launch",
        phase: "running",
        kicker: view.kicker,
        ...(view.gameTitle === undefined ? {} : { gameTitle: view.gameTitle }),
        ...(view.cart === undefined ? {} : { cart: view.cart }),
      }
    case "Paused":
      return {
        _tag: "Launch",
        phase: "paused",
        kicker: view.kicker,
        ...(view.detail === undefined ? {} : { detail: view.detail }),
        ...(view.cart === undefined ? {} : { cart: view.cart }),
      }
    case "Loading":
    case "Empty":
    case "Failed":
    case "Confirm":
    case "Problem":
    case "Ended":
      return IDLE
  }
}
