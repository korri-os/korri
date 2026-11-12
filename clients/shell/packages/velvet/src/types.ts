/**
 * Velvet's state and the words its parts speak to each other.
 *
 * Korri's SurfaceModel is never stored here: Korri owns it and replaces it
 * whole, and every function receives it beside the state. The state holds
 * only where the person is (focus, depth, what is open, what is being
 * typed) and the few games each of the home's lists shows (src/lists.ts).
 *
 * A press never acts by itself. A row, a button or a key carries an `Act`,
 * data that says what should follow; src/update.ts alone performs it. So
 * the view (which draws a row) and the update (which answers a press on it)
 * read one description of the row and can never disagree.
 */
import type {
  CatalogOrder,
  CatalogQuery,
  SurfaceGame,
  SurfaceGameplayControlValue,
  SurfaceIdentityDisposition,
  SurfaceRequest,
  SurfaceWifiAttempt,
} from "../../../../../contracts/generated/surface"

export type Game = SurfaceGame
export type Request = SurfaceRequest
export type Direction = "up" | "down" | "left" | "right"

// ---- The home's lists (src/lists.ts) ----

/** One list the home or the lobby needs, as a catalog question. */
export interface Want {
  readonly key: string
  readonly query: CatalogQuery
  readonly from: "start" | "end" | { readonly before: string }
  readonly count: number
}

export interface Lists {
  /** The answers kept, by the want's key. */
  readonly kept: Readonly<Record<string, readonly Game[]>>
  /** Games in the whole library, from the last unfiltered answer. */
  readonly total: number
  /** The sections of games on other devices, in Korri's order. */
  readonly peers: readonly string[]
  /** Games each peer section holds, when Korri has answered. */
  readonly peerTotals: Readonly<Record<string, number>>
  /** The question out, and the Ticks it has waited. */
  readonly asked?: Want
  readonly waited: number
  /** Every want up to this one in the sequence is answered. */
  readonly done: number
  /** What the lists were read against: a change makes them stale. */
  readonly stamp: string
  /** Cursors further back from the end of Korri's order, read to find more devices. */
  readonly backs: readonly string[]
  /** Cursors further back from the end of the recent order, read to find where the unplayed games begin. */
  readonly tails: readonly string[]
}

// ---- A set (the grid) ----

export interface SetState {
  /** "every", "fresh", "peer:<section>" or "system:<id>". */
  readonly key: string
  readonly title: string
  readonly order: CatalogOrder
  readonly section?: string
  readonly systemId?: string
  /** The Not started set reads the end of the recent order. */
  readonly fromEnd: boolean
  /** The focused game's place in the whole set, and its id. */
  readonly at: number
  readonly gameId?: string
  /** The first row the window shows. */
  readonly first: number
  /** Focus is on the Sort and filter pill. */
  readonly pill: boolean
  /** A question to Korri is out for this set. */
  readonly moving: boolean
  /** The first place of the set that is a game of it (Not started: the first unplayed). */
  readonly low: number
  /**
   * Not started only. True: `low` is where the unplayed games begin (a window showed the
   * last played game before it, or the start of the order). Else `low` is the lowest
   * unplayed place seen so far, and more may stand before it.
   */
  readonly lowKnown?: boolean
  /** A finger scrolled the rows (ADR 0055): the first row shown, apart from focus. */
  readonly scrolled?: number
}

export interface DetailState {
  /** The game as it was when the detail opened; the model's copy replaces it when it is in the window. */
  readonly game: Game
  readonly at: number
}

// ---- Acts ----

export type Act =
  | { readonly t: "none" }
  | { readonly t: "send"; readonly requests: readonly Request[] }
  | { readonly t: "open"; readonly page: PageRef }
  /** Leave the page (or sheet) the act came from. */
  | { readonly t: "close" }
  | { readonly t: "type"; readonly entry: Entry; readonly draft?: string }
  | { readonly t: "ask"; readonly ask: AskSpec }
  | { readonly t: "sheet"; readonly sheet: SheetKind }
  | { readonly t: "set"; readonly set: SetState }
  | { readonly t: "sort"; readonly order?: CatalogOrder; readonly systemId?: string | null }
  | { readonly t: "detail"; readonly game: Game }
  | { readonly t: "lobby" }
  | { readonly t: "candidate"; readonly rowId: string; readonly at: number }
  | { readonly t: "motion"; readonly still: boolean }
  | { readonly t: "onboarding"; readonly step: number; readonly started?: boolean; readonly finishing?: boolean }
  | { readonly t: "identity"; readonly flow: IdentityFlow }
  | { readonly t: "moving"; readonly controlId: string; readonly value: number }
  /** A control that closes the overlay was invoked: presses wait for Korri. */
  | { readonly t: "leaving" }
  /** Close every page and sheet: back to the library. */
  | { readonly t: "home" }
  | { readonly t: "seq"; readonly acts: readonly Act[] }

export const NONE: Act = { t: "none" }
export const send = (...requests: Request[]): Act => ({ t: "send", requests })
export const seq = (...acts: Act[]): Act => ({ t: "seq", acts })

// ---- Panels: every list page, sheet and the overlay ----

export interface RowSpec {
  readonly id: string
  readonly kind: "row" | "head" | "note" | "problem" | "range" | "meter"
  readonly label: string
  readonly value?: string
  readonly note?: string
  readonly mark?: "check" | "more" | "step" | "busy" | "lock" | "on" | "off"
  readonly bars?: number
  readonly dim?: boolean
  readonly danger?: boolean
  readonly share?: number
  /** What Confirm does. Absent on a row that takes no focus (a head, a note). */
  readonly press?: Act
  /** What left (-1) and right (1) do. */
  readonly step?: (direction: -1 | 1) => Act
  /** A finger on a range: its place along the track, and whether it lifted. */
  readonly slide?: (at: number, released: boolean) => Act
  /** Takes focus though Confirm does nothing (a fact worth reading whole). */
  readonly rests?: boolean
}

export interface ButtonSpec {
  readonly id: string
  readonly label: string
  readonly icon?: string
  readonly note?: string
  readonly quiet?: boolean
  readonly dim?: boolean
  readonly danger?: boolean
  readonly press: Act
}

export interface PanelSide {
  readonly kind: "qr" | "text" | "tile"
  readonly title?: string
  readonly lines?: readonly string[]
  readonly qr?: string
  readonly game?: Game
}

export interface PanelSpec {
  /** Names the panel: its focus is kept under this key. */
  readonly key: string
  readonly title: string
  readonly sub?: string
  readonly rows: readonly RowSpec[]
  readonly buttons: readonly ButtonSpec[]
  readonly side?: PanelSide
  /** What Back does. */
  readonly back: Act
  /** What the menu button does; absent: nothing. */
  readonly menu?: Act
  /** First boot's place among its steps. */
  readonly step?: number
  readonly steps?: number
  /** Where focus lands when the panel opens: a row or button id. */
  readonly first?: string
}

export interface PanelState {
  /** A row id, or a button id. */
  readonly focus: string
  /** The focused row's index, so focus stays put when Korri takes the row away. */
  readonly at: number
}

export type PageRef =
  | { readonly p: "settings" }
  | { readonly p: "wifi" }
  | { readonly p: "store" }
  | { readonly p: "provider"; readonly id: string }
  | { readonly p: "entry" }
  | { readonly p: "invites" }
  | { readonly p: "identity" }

export type SheetKind = "menu" | "sort" | "actions"

// ---- The keyboard ----

export type KeyLayer = "letters" | "shift" | "symbols"

export interface NumberRange {
  readonly min?: number
  readonly max?: number
  readonly step?: number
  readonly clearable: boolean
}

export type Entry =
  | { readonly e: "setting"; readonly settingId: string; readonly label: string; readonly secret: boolean; readonly maxLength?: number; readonly clearLabel?: string; readonly placeholder?: string; readonly numeric?: NumberRange; readonly allowEmpty?: boolean }
  | { readonly e: "wifiPassword"; readonly networkId: string; readonly ssid: string }
  | { readonly e: "hiddenSsid" }
  | { readonly e: "hiddenPassword"; readonly ssid: string }
  | { readonly e: "storeSearch"; readonly providerId: string; readonly provider: string }
  | { readonly e: "backupPassword"; readonly retiredPublicKey?: string }
  | { readonly e: "switchSecret" }
  | { readonly e: "switchPassword"; readonly encryptedSecret: string }
  | { readonly e: "bunker" }

export interface KeyboardState {
  readonly entry: Entry
  readonly draft: string
  readonly caret: number
  readonly layer: KeyLayer
  /** The focused key's id. */
  readonly key: string
}

// ---- Velvet's own question ----

export interface AskButton {
  readonly label: string
  readonly danger?: boolean
  readonly act: Act
}

export interface AskSpec {
  readonly title: string
  readonly lines: readonly string[]
  /** The first button is the way out, and focus starts on it. */
  readonly buttons: readonly AskButton[]
}

export interface AskState {
  readonly spec: AskSpec
  readonly at: number
}

// ---- Identity (the switch needs several answers before it is sent) ----

export type IdentitySource =
  | { readonly kind: "backup"; readonly encryptedSecret: string; readonly password: string }
  | { readonly kind: "bunker"; readonly bunkerUri: string }

export type IdentityFlow =
  | { readonly f: "idle" }
  /** A backup was asked for: Korri's Working, then its BackupReady, is the page. */
  | { readonly f: "backup" }
  | { readonly f: "disposition"; readonly source: IdentitySource }
  | { readonly f: "trust"; readonly source: IdentitySource; readonly disposition: SurfaceIdentityDisposition }

// ---- The gameplay overlay ----

export interface OverlayState {
  /** A range's value ahead of Korri, shown while it moves. */
  readonly moving: Readonly<Record<string, number>>
  /** A control that closes the overlay was invoked: presses wait for Korri. */
  readonly leaving: boolean
}

export interface State {
  readonly size: { readonly width: number; readonly height: number }
  /** Reduced motion (the velvet.motion preference). */
  readonly still: boolean
  /** A tap or a drag came last, not a button (ADR 0055). */
  readonly touched: boolean
  /** The stage has rested long enough to drift. */
  readonly drift: boolean
  readonly lists: Lists
  /** 0: the pick. 1: the row under it. 2: the lobby. */
  readonly depth: 0 | 1 | 2
  /** On the home's top line: the menu pill holds focus. */
  readonly onMenu: boolean
  readonly rowAt: number
  /** The focused shelf's key, and each shelf's column. */
  readonly shelf: string
  readonly columns: Readonly<Record<string, number>>
  /** Where a finger scrolled the row and the shelves (ADR 0055): the column each keeps in view, apart from focus. */
  readonly scrolled?: Readonly<Record<string, number>>
  readonly set?: SetState
  readonly detail?: DetailState
  readonly sheet?: { readonly kind: SheetKind }
  readonly pages: readonly PageRef[]
  /** Each panel's focus, by the panel's key. */
  readonly panels: Readonly<Record<string, PanelState>>
  /** A candidate on a choice Korri publishes with nothing chosen. */
  readonly candidate?: { readonly rowId: string; readonly at: number }
  readonly keyboard?: KeyboardState
  readonly ask?: AskState
  /** Korri's status: which button holds focus, the first line shown, and what it is for. */
  readonly curtain: { readonly key: string; readonly at: number; readonly line: number }
  readonly onboarding: { readonly step: number; readonly started: boolean; readonly finishing: boolean }
  readonly identity: IdentityFlow
  readonly overlay: OverlayState
  /** The last Wi-Fi attempt seen, so a fresh rejected key reopens the keyboard once. */
  readonly attempt: SurfaceWifiAttempt
  /** The game Korri last heard has focus (FocusGame). */
  readonly reported: string | null
  /** Invites already announced, and the one the notice shows with the Ticks it has left. */
  readonly invitesSeen: readonly string[]
  readonly notice?: { readonly id: string; readonly left: number }
  /** Whether Korri's status was Browsing at the last model: a return to it refreshes the lists. */
  readonly wasBrowsing: boolean
}

export type GameplayValue = SurfaceGameplayControlValue
