/**
 * The order desk: getting a game from a provider, by mail order.
 *
 * The desk is the third use of the door board (AGENTS.md): the store journey
 * needs typing and must work from an empty library, and the 3D room can offer
 * neither. Its facts are Korri's, converted once in boxbuster-store-view.ts
 * into the shapes below; where you are at the desk is Boxbuster's (`DeskPlace`).
 * Pure: the rules for Back and for the keys live here and are tested with
 * plain values.
 */

export interface DeskAction {
  readonly id: string
  readonly label: string
  readonly enabled: boolean
}

export interface DeskEntry {
  readonly id: string
  readonly title: string
  readonly system?: string
}

export type DeskProviders =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Problem"; readonly message: string; readonly actions: readonly DeskAction[] }
  | { readonly _tag: "Ready"; readonly providers: readonly { readonly id: string; readonly title: string }[] }

export type DeskSearch =
  | { readonly _tag: "Idle" }
  | { readonly _tag: "Searching"; readonly providerId: string; readonly query: string }
  | { readonly _tag: "Problem"; readonly providerId: string; readonly query: string; readonly message: string; readonly actions: readonly DeskAction[] }
  | { readonly _tag: "Ready"; readonly providerId: string; readonly query: string; readonly entries: readonly DeskEntry[] }

export type DeskGet =
  | { readonly _tag: "Offer"; readonly note?: string; readonly actions: readonly DeskAction[] }
  | { readonly _tag: "Unavailable"; readonly reason: string }
  | { readonly _tag: "Confirm"; readonly kicker: string; readonly detail?: string; readonly actions: readonly DeskAction[] }
  | {
      readonly _tag: "Working"
      readonly kicker: string
      readonly detail?: string
      readonly progress?: { readonly done: number; readonly total?: number }
      readonly actions: readonly DeskAction[]
    }
  | { readonly _tag: "InLibrary"; readonly detail?: string; readonly gameId?: string }
  | { readonly _tag: "Problem"; readonly kicker: string; readonly reason: string; readonly actions: readonly DeskAction[] }

export type DeskDetails =
  | { readonly _tag: "Closed" }
  | { readonly _tag: "Loading"; readonly entry: DeskEntry }
  | { readonly _tag: "Problem"; readonly entry: DeskEntry; readonly message: string; readonly actions: readonly DeskAction[] }
  | { readonly _tag: "Ready"; readonly entry: DeskEntry; readonly description?: string; readonly get: DeskGet }

export interface DeskStore {
  readonly providers: DeskProviders
  readonly search: DeskSearch
  readonly details: DeskDetails
}

/** Where you are at the desk: a page, which provider, and the order form. */
export interface DeskPlace {
  readonly view: "Providers" | "Form" | "Results" | "Entry"
  readonly providerId?: string
  /** What is written on the order form. Kept while you look at results. */
  readonly text: string
}

export const DESK_START: DeskPlace = { view: "Providers", text: "" }

/** The longest order the form takes; a provider is asked for words. */
export const DESK_TEXT_MAX = 64

/** The form's keys, row by row, as the desk prints them. */
export const DESK_KEYS: readonly (readonly string[])[] = [
  ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
  ["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"],
  ["a", "s", "d", "f", "g", "h", "j", "k", "l", "-"],
  ["z", "x", "c", "v", "b", "n", "m", "'", ".", ":"],
]

/** Back at the desk: one page at a time, then away from the desk. Back
 * never cancels: a download keeps going and shows on its entry. */
export function deskBack(place: DeskPlace, searched: boolean): DeskPlace | "Leave" {
  switch (place.view) {
    case "Form":
      return { ...place, view: searched ? "Results" : "Providers" }
    case "Entry":
      return { ...place, view: "Results" }
    case "Results":
      return { ...place, view: "Providers" }
    case "Providers":
      return "Leave"
  }
}

/** A key on the form. Past the limit, nothing more is written. */
export function deskType(place: DeskPlace, character: string): DeskPlace {
  return [...place.text].length >= DESK_TEXT_MAX ? place : { ...place, text: place.text + character }
}

export const deskDelete = (place: DeskPlace): DeskPlace => ({ ...place, text: [...place.text].slice(0, -1).join("") })

/** The search Korri holds for this provider, or none. */
export function deskSearchFor(store: DeskStore, providerId: string | undefined): DeskSearch {
  const search = store.search
  return search._tag !== "Idle" && search.providerId === providerId ? search : { _tag: "Idle" }
}

/** "16 of 32 sixteenths", as the bar draws it: whole cells of `cells`. */
export function deskCells(progress: { readonly done: number; readonly total?: number }, cells: number): number | undefined {
  if (progress.total === undefined || progress.total <= 0) return undefined
  return Math.max(0, Math.min(cells, Math.floor((progress.done / progress.total) * cells)))
}
