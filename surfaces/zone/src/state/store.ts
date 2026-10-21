/**
 * GET GAMES: getting a game from a provider, as one value and the one
 * function that changes it.
 *
 *     open      the GET GAMES page holds the screen
 *     page      the providers, one provider's search and results, or one entry
 *     provider  the provider whose results are open
 *     keyboard  the search draft while the keyboard is up, as FIND's
 *     focused   the row the cursor is on (src/plan/store.ts names them)
 *     dialog    the button the cursor is on in Korri's question or problem
 *
 * Every fact on the page is Korri's (`SurfaceModel.store`): the providers,
 * the results, the entry and where getting it stands, with Korri's own
 * actions. This model holds only where the person is. A download keeps going
 * while the person looks elsewhere: Back never cancels it, and nothing here
 * asks Korri to stop.
 */
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import {
  ZONE_STORE_QUERY_MAX,
  ZONE_STORE_SEARCH_ROW,
  zoneStoreActionLive,
  zoneStoreEntryRow,
  zoneStoreProviderRow,
  zoneStoreSearch,
  type ZoneStorePage,
} from "../plan/store"
import { pressFindKey, ZONE_FIND_START, type ZoneFind, type ZoneFindKey } from "./games"
import { ask, stay, type ZoneStep } from "./requests"

export interface ZoneStoreModel {
  readonly open: boolean
  readonly page: ZoneStorePage
  readonly provider: string | undefined
  readonly keyboard: ZoneFind | undefined
  readonly focused: string | undefined
  /** The index of the button the cursor is on in Korri's question or problem. */
  readonly dialog: number
}

export const ZONE_STORE_START: ZoneStoreModel = {
  open: false,
  page: "providers",
  provider: undefined,
  keyboard: undefined,
  focused: undefined,
  dialog: 0,
}

export type ZoneStoreMessage =
  /** GET GAMES on the GAMES page: the page opens and Korri reads its providers again. */
  | { readonly _tag: "OpenedStore" }
  /** Korri published a new model. */
  | { readonly _tag: "KorriPublished" }
  | { readonly _tag: "PressedBack" }
  /** The cursor is on a row or a button; `rowId` as src/plan/store.ts names it. */
  | { readonly _tag: "FocusedRow"; readonly rowId: string }
  /** A on a row or a button. */
  | { readonly _tag: "PressedRow"; readonly rowId: string }
  | { readonly _tag: "FocusedKey"; readonly id: string }
  | { readonly _tag: "PressedKey"; readonly key: ZoneFindKey }
  /** The cursor is on a button of Korri's question or problem. */
  | { readonly _tag: "FocusedDialogButton"; readonly index: number }
  /** A on one of Korri's actions, wherever it stands. */
  | { readonly _tag: "PressedAction"; readonly actionId: string }

export function updateStore(
  model: ZoneStoreModel,
  message: ZoneStoreMessage,
  korri: Pick<SurfaceModel, "store">,
): ZoneStep<ZoneStoreModel> {
  const store = korri.store
  // A host that cannot get games has no GET GAMES page.
  if (store === undefined) return stay(model === ZONE_STORE_START ? model : ZONE_STORE_START)
  switch (message._tag) {
    case "OpenedStore":
      return ask({ ...ZONE_STORE_START, open: true }, { _tag: "OpenStore" })
    case "KorriPublished":
      return stay(model)
    case "PressedBack":
      return stay(back(model, store))
    case "FocusedRow":
      return model.focused === message.rowId ? stay(model) : stay({ ...model, focused: message.rowId })
    case "FocusedDialogButton":
      return stay({ ...model, dialog: Math.max(0, message.index) })
    case "FocusedKey":
      return model.keyboard === undefined ? stay(model) : stay({ ...model, keyboard: { ...model.keyboard, key: message.id } })
    case "PressedKey": {
      const keyboard = model.keyboard
      if (keyboard === undefined || model.provider === undefined) return stay(model)
      // DONE asks the provider on screen for what is typed; the keyboard goes down.
      if (message.key.kind === "done") {
        return ask(
          { ...model, keyboard: undefined, focused: ZONE_STORE_SEARCH_ROW },
          { _tag: "SearchStore", providerId: model.provider, query: keyboard.draft.trim() },
        )
      }
      const next = pressFindKey(keyboard, message.key)
      return stay({ ...model, keyboard: next.draft.length > ZONE_STORE_QUERY_MAX ? keyboard : next })
    }
    case "PressedAction":
      return zoneStoreActionLive(store, message.actionId)
        ? ask({ ...model, dialog: 0 }, { _tag: "RunAction", actionId: message.actionId })
        : stay(model)
    case "PressedRow":
      return pressRow(model, message.rowId, store)
  }
}

function pressRow(
  model: ZoneStoreModel,
  rowId: string,
  store: NonNullable<SurfaceModel["store"]>,
): ZoneStep<ZoneStoreModel> {
  const [kind, ...rest] = rowId.split(":")
  const id = rest.join(":")
  switch (kind) {
    case "provider": {
      const listed = store.providers._tag === "Ready" && store.providers.providers.some(item => item.id === id)
      if (!listed) return stay(model)
      // A provider not asked yet opens on the keyboard: typing is why you came.
      const asked = zoneStoreSearch(store, id)._tag !== "Idle"
      return stay({
        ...model,
        page: "results",
        provider: id,
        keyboard: asked ? undefined : ZONE_FIND_START,
        focused: asked ? ZONE_STORE_SEARCH_ROW : undefined,
      })
    }
    case ZONE_STORE_SEARCH_ROW: {
      const search = zoneStoreSearch(store, model.provider)
      const draft = search._tag === "Idle" ? "" : search.query
      return stay({ ...model, keyboard: { ...ZONE_FIND_START, draft, caret: draft.length } })
    }
    case "entry": {
      const search = zoneStoreSearch(store, model.provider)
      if (search._tag !== "Ready" || !search.entries.some(entry => entry.id === id)) return stay(model)
      return ask({ ...model, page: "entry", focused: undefined, dialog: 0 }, { _tag: "OpenStoreEntry", entryId: id })
    }
    case "action":
      return zoneStoreActionLive(store, id) ? ask({ ...model, dialog: 0 }, { _tag: "RunAction", actionId: id }) : stay(model)
    case "play": {
      const details = store.details
      if (details._tag !== "Ready" || details.get._tag !== "InLibrary" || details.get.gameId !== id) return stay(model)
      // The game is in the library: the page closes and the game starts, so
      // GAMES is where the person comes back to.
      return ask({ ...ZONE_STORE_START }, { _tag: "LaunchGame", gameId: id })
    }
    default:
      return stay(model)
  }
}

/** The keyboard, then the entry, then the results, then the page itself. */
function back(model: ZoneStoreModel, store: NonNullable<SurfaceModel["store"]>): ZoneStoreModel {
  if (!model.open) return model
  if (model.keyboard !== undefined) return { ...model, keyboard: undefined, focused: ZONE_STORE_SEARCH_ROW }
  switch (model.page) {
    case "entry":
      // The cursor goes back to the entry it came from.
      return {
        ...model,
        page: "results",
        focused: store.details._tag === "Closed" ? undefined : zoneStoreEntryRow(store.details.entry.id),
        dialog: 0,
      }
    case "results":
      return {
        ...model,
        page: "providers",
        focused: model.provider === undefined ? undefined : zoneStoreProviderRow(model.provider),
      }
    case "providers":
      return ZONE_STORE_START
  }
}
