import type { SurfaceAction, SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
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
      /** Bytes done and, when Korri knows it, bytes in all: a real bar. */
      readonly progress?: { readonly done: number; readonly total?: number }
      readonly actions?: readonly SurfaceAction[]
      readonly cart?: PicoLaunchCart
    }
  /** Korri asks before it acts. The actions are Korri's; Pico renders them.
   * A save split (ChooseSave) asks the same way: one answer per side first. */
  | {
      readonly _tag: "Confirm"
      readonly kicker: string
      readonly detail?: string
      readonly actions: readonly SurfaceAction[]
      /** The action that declines; the cursor starts on it. */
      readonly declineActionId?: string
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
  /** This device plays on another device's stream, and that device paused
   * the session. The kicker names that device; the actions are Korri's. */
  | {
      readonly _tag: "Paused"
      readonly kicker: string
      readonly detail?: string
      readonly actions: readonly SurfaceAction[]
      readonly cart?: PicoLaunchCart
    }
  /** The session this device played on ended. `reason` is Korri's words. */
  | {
      readonly _tag: "Ended"
      readonly kicker: string
      readonly reason: string
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
        ...(status.progress === undefined ? {} : { progress: status.progress }),
        ...(status.actions === undefined ? {} : { actions: status.actions }),
        ...(cart === undefined ? {} : { cart }),
      }
    }
    case "Confirm": {
      const cart = cartFor(model, status.gameId)
      return {
        _tag: "Confirm",
        kicker: status.kicker,
        ...(status.detail === undefined ? {} : { detail: status.detail }),
        actions: status.actions,
        ...(status.declineActionId === undefined ? {} : { declineActionId: status.declineActionId }),
        ...(cart === undefined ? {} : { cart }),
      }
    }
    case "ChooseSave": {
      const cart = cartFor(model, status.gameId)
      return {
        _tag: "Confirm",
        kicker: status.kicker,
        ...(status.detail === undefined ? {} : { detail: status.detail }),
        actions: picoChooseSaveActions(status),
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
    case "Paused": {
      const cart = cartFor(model, status.gameId)
      return {
        _tag: "Paused",
        kicker: status.kicker,
        ...(status.detail === undefined ? {} : { detail: status.detail }),
        actions: status.actions,
        ...(cart === undefined ? {} : { cart }),
      }
    }
    // Korri's reason already names the game and the device; the title would
    // say it twice.
    case "Ended":
      return { _tag: "Ended", kicker: status.kicker, reason: status.reason }
    case "Browsing":
      return picoHomeViewFromCatalog(model.catalog)
  }
}

const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"]
const two = (n: number) => String(n).padStart(2, "0")

/** "OCT 21 09:18": when a side's session ended, by that device's clock.
 * Display only; Pico never compares it. */
export function picoSaveTime(at: number): string {
  const date = new Date(at)
  if (Number.isNaN(date.getTime())) return "--"
  return `${MONTHS[date.getMonth()] ?? ""} ${date.getDate()} ${two(date.getHours())}:${two(date.getMinutes())}`
}

/**
 * A save split as Confirm answers: one per side (its device and end time,
 * running the side's own action id), then Korri's other actions (Not now).
 */
export function picoChooseSaveActions(
  status: Extract<SurfaceStatus, { _tag: "ChooseSave" }>,
): readonly SurfaceAction[] {
  return [
    ...status.sides.map(side => ({
      id: side.actionId,
      label: `${side.device}, ${picoSaveTime(side.endedAt)}`,
      enabled: true,
    })),
    ...status.actions,
  ]
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
