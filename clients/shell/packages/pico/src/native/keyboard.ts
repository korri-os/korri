/**
 * Pico's on-screen keyboard (PicoKeyboard): ten columns, the characters in
 * four rows, then SPACE, DEL and CLEAR after the digits, and for text the
 * CAPS and symbols keys under them. Each key is a focus target; logic.js
 * places them so main.slint only draws.
 */
import type { PicoTextDraft } from "../pico-text-draft"
import type { Box, Target } from "./focus"
import type { KeyView } from "./kit"

const SEARCH_ROWS = ["ABCDEFGHIJ", "KLMNOPQRST", "UVWXYZ0123", "456789"] as const
const LETTER_ROWS = ["abcdefghij", "klmnopqrst", "uvwxyz0123", "456789"] as const
const SYMBOL_ROWS = ["!\"#$%&'()*", "+,-./:;<=>", "?@[\\]^_`{|", "}~"] as const

export type KeyPress =
  | { readonly _tag: "Type"; readonly character: string }
  | { readonly _tag: "Backspace" }
  | { readonly _tag: "Clear" }
  | { readonly _tag: "Capitals" }
  | { readonly _tag: "Symbols" }

interface Placed {
  readonly id: string
  readonly cap: string
  readonly row: number
  readonly col: number
  readonly cols: number
  readonly press: KeyPress
  readonly lit: boolean
}

function layout(charset: "search" | "text", draft: Pick<PicoTextDraft, "capitals" | "symbols">): readonly Placed[] {
  const rows = charset === "search"
    ? SEARCH_ROWS
    : draft.symbols ? SYMBOL_ROWS : draft.capitals ? LETTER_ROWS.map(row => row.toUpperCase()) : LETTER_ROWS
  const keys: Placed[] = []
  rows.forEach((characters, row) => {
    ;[...characters].forEach((character, col) => {
      keys.push({ id: `key:${row}:${col}`, cap: character, row, col, cols: 1, press: { _tag: "Type", character }, lit: false })
    })
  })
  // After the last row's characters: SPACE (two columns), DEL, CLEAR.
  keys.push({ id: "key:space", cap: "SPACE", row: 3, col: 6, cols: 2, press: { _tag: "Type", character: " " }, lit: false })
  keys.push({ id: "key:del", cap: "DEL", row: 3, col: 8, cols: 1, press: { _tag: "Backspace" }, lit: false })
  keys.push({ id: "key:clear", cap: "CLEAR", row: 3, col: 9, cols: 1, press: { _tag: "Clear" }, lit: false })
  if (charset === "text") {
    keys.push({ id: "key:caps", cap: "↑ CAPS", row: 4, col: 0, cols: 5, press: { _tag: "Capitals" }, lit: draft.capitals })
    keys.push({ id: "key:symbols", cap: draft.symbols ? "abc" : "#+=", row: 4, col: 5, cols: 5, press: { _tag: "Symbols" }, lit: draft.symbols })
  }
  return keys
}

export interface Keyboard<Act> {
  readonly keys: readonly KeyView[]
  readonly rows: number
  readonly targets: readonly Target<Act>[]
}

/**
 * The keys as views and focus targets. `top` is where the first row stands
 * on the screen's focus plan; a row is ten units tall and a column ten wide,
 * spanning `width` units.
 */
export function keyboard<Act>(
  charset: "search" | "text",
  draft: Pick<PicoTextDraft, "capitals" | "symbols">,
  focused: string | undefined,
  top: number,
  act: (press: KeyPress) => Act,
): Keyboard<Act> {
  const placed = layout(charset, draft)
  const box = (key: Placed): Box => ({ x: key.col * 10, y: top + key.row * 10, w: key.cols * 10, h: 8 })
  return {
    keys: placed.map(key => ({ cap: key.cap, focused: key.id === focused, focusId: key.id, lit: key.lit, row: key.row, col: key.col, cols: key.cols })),
    rows: charset === "text" ? 5 : 4,
    targets: placed.map(key => ({ id: key.id, region: "keys", box: box(key), act: act(key.press) })),
  }
}

/** The first key, where a keyboard's cursor starts. */
export const FIRST_KEY = "key:0:0"
