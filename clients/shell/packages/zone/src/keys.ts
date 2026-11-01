/**
 * The slanted keyboard as a grid the cursor walks, for every keyboard Zone
 * draws (FIND, OPTIONS' entry, SAVE DATA's, first boot's NAME, GET GAMES'
 * search). In the browser the host moved DOM focus between the keys by
 * geometry (clients/portal/src/input/spatial-focus.ts); here the surface
 * moves it, on the same rows ZoneKeyboard draws (src/plan/keyboard.ts).
 *
 * Keys are laid out in units: a character key is one, the control keys take
 * the units ZONE_KEYBOARD_CONTROLS gives them. Each row is centred on the
 * widest, as the panel centres its rows. Left and right step along a row
 * and stop at its ends; up and down go to the key in the next row whose
 * centre is nearest.
 */
import type { ZoneFindKey, ZoneFindLayer } from "./state/games"
import { ZONE_KEYBOARD_CONTROLS, ZONE_KEYBOARD_ROWS, ZONE_KEYBOARD_UNITS } from "./plan/keyboard"

export interface ZoneGridKey {
  readonly id: string
  readonly label: string
  readonly key: ZoneFindKey | { readonly kind: "clear" }
  readonly units: number
  /** Lower case, so Zone Caps cannot draw it: the fact face. */
  readonly fact: boolean
  /** The row it stands in, 0 the digits, 4 the control row. */
  readonly row: number
  /** Its left edge in units, the row centred on the widest. */
  readonly at: number
}

const LAYERS: readonly ZoneFindLayer[] = ["letters", "shift", "symbols", "uri"]
const LAYER_LABELS: Readonly<Record<ZoneFindLayer, string>> = { letters: "abc", shift: "ABC", symbols: "#?!", uri: "://" }

/** Every key of a layer, rows top to bottom; `clear` adds Korri's clear key at the end. */
export function keyGrid(layer: ZoneFindLayer, clear?: string): readonly ZoneGridKey[] {
  const rows: Omit<ZoneGridKey, "row" | "at">[][] = ZONE_KEYBOARD_ROWS[layer].map(row =>
    [...row].map(char => ({ id: char, label: char, key: { kind: "char", char } as const, units: 1, fact: char !== char.toUpperCase() })))
  const units = (id: string) => ZONE_KEYBOARD_CONTROLS.find(control => control.id === id)?.units ?? 1
  rows.push([
    ...LAYERS.map(name => ({
      id: `layer:${name}`,
      label: LAYER_LABELS[name],
      key: { kind: "layer", layer: name } as const,
      units: units(`layer:${name}`),
      fact: name === "letters",
    })),
    { id: "space", label: "Space", key: { kind: "space" }, units: units("space"), fact: false },
    { id: "delete", label: "Delete", key: { kind: "delete" }, units: units("delete"), fact: false },
    { id: "done", label: "Done", key: { kind: "done" }, units: units("done"), fact: false },
    ...(clear === undefined ? [] : [{ id: "clear", label: clear, key: { kind: "clear" } as const, units: 2, fact: false }]),
  ])
  const widest = Math.max(ZONE_KEYBOARD_UNITS, ...rows.map(row => row.reduce((sum, key) => sum + key.units, 0)))
  return rows.flatMap((row, index) => {
    const span = row.reduce((sum, key) => sum + key.units, 0)
    let at = (widest - span) / 2
    return row.map(key => {
      const placed = { ...key, row: index, at }
      at += key.units
      return placed
    })
  })
}

/** The key the cursor is on: `id` when the layer has it, else the first letter row's first key. */
export function keyAt(grid: readonly ZoneGridKey[], id: string | undefined): ZoneGridKey {
  return grid.find(key => key.id === id) ?? grid.find(key => key.row === 1) ?? grid[0]!
}

/** The key a direction leads to from `id`; the same key at an edge. */
export function keyMove(grid: readonly ZoneGridKey[], id: string | undefined, direction: "up" | "down" | "left" | "right"): ZoneGridKey {
  const from = keyAt(grid, id)
  const row = grid.filter(key => key.row === from.row)
  const index = row.indexOf(from)
  if (direction === "left") return row[Math.max(0, index - 1)] ?? from
  if (direction === "right") return row[Math.min(row.length - 1, index + 1)] ?? from
  const target = from.row + (direction === "up" ? -1 : 1)
  const next = grid.filter(key => key.row === target)
  if (next.length === 0) return from
  const centre = from.at + from.units / 2
  return next.reduce((best, key) =>
    Math.abs(key.at + key.units / 2 - centre) < Math.abs(best.at + best.units / 2 - centre) ? key : best)
}

/** Zone's key as the keyboard reports it; `clear` is not one. */
export const findKey = (key: ZoneGridKey["key"]): ZoneFindKey | undefined => (key.kind === "clear" ? undefined : key)
