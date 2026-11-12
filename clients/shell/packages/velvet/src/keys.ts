/**
 * Velvet's keyboard: five rows of ten columns, every key on the grid, so a
 * press moves to the key drawn that way (src/focus.ts) and the plan sizes
 * one key for every layer. Three layers: letters, capitals, and symbols
 * (every ASCII mark, so a Wi-Fi key, a bunker URI and a backup's secret can
 * all be typed).
 *
 * The bottom row holds the two other layers, space, delete and Done; a
 * setting Korri lets a person clear gets Korri's clear key beside Done.
 */
import { nearest, type Box } from "./focus"
import { KEY_COLUMNS } from "./plan"
import type { Direction, KeyLayer } from "./types"

export type KeyKind =
  | { readonly kind: "char"; readonly char: string }
  | { readonly kind: "space" }
  | { readonly kind: "delete" }
  | { readonly kind: "done" }
  | { readonly kind: "clear" }
  | { readonly kind: "layer"; readonly layer: KeyLayer }

export interface GridKey {
  readonly id: string
  readonly label: string
  readonly icon: string
  readonly key: KeyKind
  readonly row: number
  /** Its left edge and its width, in columns. */
  readonly column: number
  readonly span: number
}

const ROWS: Readonly<Record<KeyLayer, readonly string[]>> = {
  letters: ["1234567890", "qwertyuiop", "asdfghjkl'", "zxcvbnm,.-"],
  shift: ["1234567890", "QWERTYUIOP", "ASDFGHJKL'", "ZXCVBNM,.-"],
  symbols: ["1234567890", "!@#$%^&*()", "-_=+[]{}\\|", ":;/?\"<>~`."],
}

const LAYER_LABEL: Readonly<Record<KeyLayer, string>> = { letters: "abc", shift: "ABC", symbols: "#+=" }

/** Every key of a layer. `clear` adds Korri's clear key. */
export function keyGrid(layer: KeyLayer, clear?: string): readonly GridKey[] {
  const keys: GridKey[] = []
  ROWS[layer].forEach((row, index) => {
    [...row].forEach((char, column) => keys.push({ id: `k:${char}`, label: char, icon: "", key: { kind: "char", char }, row: index, column, span: 1 }))
  })
  const others = (["letters", "shift", "symbols"] as const).filter(other => other !== layer)
  const controls: Omit<GridKey, "row" | "column">[] = [
    ...others.map(other => ({ id: `k:layer:${other}`, label: LAYER_LABEL[other], icon: "", key: { kind: "layer", layer: other } as const, span: clear === undefined ? 1.5 : 1.25 })),
    { id: "k:space", label: "Space", icon: "", key: { kind: "space" }, span: clear === undefined ? 3 : 2.5 },
    { id: "k:delete", label: "Delete", icon: "", key: { kind: "delete" }, span: clear === undefined ? 2 : 1.5 },
    ...(clear === undefined ? [] : [{ id: "k:clear", label: clear, icon: "", key: { kind: "clear" } as const, span: 1.5 }]),
    { id: "k:done", label: "Done", icon: "", key: { kind: "done" }, span: 2 },
  ]
  let column = 0
  for (const control of controls) {
    keys.push({ ...control, row: ROWS[layer].length, column })
    column += control.span
  }
  return keys
}

const boxes = (grid: readonly GridKey[], keyW: number, keyH: number, gap: number): Box[] =>
  grid.map(key => ({ id: key.id, x: key.column * (keyW + gap), y: key.row * (keyH + gap), w: key.span * (keyW + gap) - gap, h: keyH }))

/** The key the cursor is on: `id` when the layer has it, else Done. */
export const keyAt = (grid: readonly GridKey[], id: string | undefined): GridKey =>
  grid.find(key => key.id === id) ?? grid.find(key => key.id === "k:done") ?? grid[0]!

/** The key a direction leads to; the same key at an edge. */
export function keyMove(grid: readonly GridKey[], id: string, direction: Direction, keyW: number, keyH: number, gap: number): string {
  return nearest(boxes(grid, keyW, keyH, gap), keyAt(grid, id).id, direction) ?? keyAt(grid, id).id
}

/** After a layer change: the key standing where the focused one stood. */
export function keyInPlace(from: GridKey, grid: readonly GridKey[]): string {
  const row = grid.filter(key => key.row === from.row)
  const centre = from.column + from.span / 2
  const found = row.reduce<GridKey | undefined>((best, key) =>
    best === undefined || Math.abs(key.column + key.span / 2 - centre) < Math.abs(best.column + best.span / 2 - centre) ? key : best, undefined)
  return (found ?? keyAt(grid, undefined)).id
}

export const KEY_GRID_COLUMNS = KEY_COLUMNS
