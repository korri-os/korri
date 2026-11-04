/**
 * How far the probe's focus walk must go to see everything it would see.
 *
 * The probe (clients/shell/examples/surface-probe.rs, `walk`) explores
 * breadth first. Each distinct view it reaches costs three renders, about
 * a second of wall time at every size. Many views repeat a focus target
 * already seen: the same control, on the same screen, with other facts
 * changed. This file runs the same walk on the logic alone, in
 * milliseconds, at each window size: the shell's `Resized` (contract A2)
 * follows `init`. It finds the fewest states that still reach every focus
 * target the full walk reaches.
 *
 * The probe's walk is mirrored step by step:
 * - fresh logic per path;
 * - korrid's catalog windows (clients/shell/src/catalog.rs);
 * - FocusGame with `fixtureGameActions`;
 * - views de-duplicated by their JSON;
 * - children queued only from new views.
 */
import { init, overlay, update, view } from "../src/native/program"
import type { KorriModel } from "../src/model"
import type { State } from "../src/native/types"
import { PICO_DEFAULT_FONT } from "../src/pico-fonts"
import { PICO_FONT_KEY } from "../src/pico-font-preference"

type Json = Record<string, unknown>
type Game = Json & { readonly id: string; readonly title: string }

const PRESSES = ["up", "down", "left", "right", "confirm", "back", "menu", "options"] as const
const DEFAULT_COUNT = 50
const MAX_COUNT = 200

const num = (game: Game, key: string) => (typeof game[key] === "number" ? (game[key] as number) : -Infinity)

/** catalog.rs `select`. */
function select(games: readonly Game[], query: Json): Game[] {
  const text = String(query["text"] ?? "").trim().toLowerCase()
  const section = typeof query["section"] === "string" ? query["section"] : undefined
  const selected = games.filter(
    game => (text === "" || game.title.toLowerCase().includes(text)) && (section === undefined || game["section"] === section),
  )
  switch (query["order"]) {
    case "title":
      return selected.sort((a, b) => (a.title.toLowerCase() < b.title.toLowerCase() ? -1 : a.title.toLowerCase() > b.title.toLowerCase() ? 1 : 0))
    case "mostPlayed":
      return selected.sort((a, b) => num(b, "playCount") - num(a, "playCount"))
    case "recent":
      return selected.sort((a, b) => num(b, "lastPlayedAt") - num(a, "lastPlayedAt"))
    default:
      return selected
  }
}

const cursorAt = (cursor: unknown) => (typeof cursor === "string" && /^i\d+$/.test(cursor) ? Number(cursor.slice(1)) : undefined)

/** catalog.rs `window`. */
function window(games: readonly Game[], query: Json, from: Json, wanted: number): Json {
  if (games.length === 0) return { _tag: "Empty" }
  const count = Math.max(1, Math.min(MAX_COUNT, wanted))
  const selected = select(games, query)
  const total = selected.length
  let start = 0
  switch (from["_tag"]) {
    case "End":
      start = Math.max(0, total - count)
      break
    case "After": {
      const at = cursorAt(from["cursor"])
      start = at === undefined ? 0 : Math.min(at + 1, total)
      break
    }
    case "Before": {
      const at = cursorAt(from["cursor"])
      start = at === undefined ? 0 : Math.max(0, Math.min(at, total) - count)
      break
    }
    case "Around": {
      const at = selected.findIndex(game => game.id === from["gameId"])
      start = at < 0 ? 0 : Math.min(Math.max(0, at - Math.floor(count / 2)), Math.max(0, total - count))
      break
    }
  }
  const end = Math.min(start + count, total)
  return {
    _tag: "Ready",
    total,
    window: selected.slice(start, end),
    firstIndex: start,
    query,
    ...(start > 0 ? { before: `i${start}` } : {}),
    ...(end < total ? { after: `i${end - 1}` } : {}),
  }
}

/** The probe's korrid: one model file, the catalog windows, the focused game. */
class Korri {
  readonly base: Json
  readonly games: readonly Game[]
  readonly fixed: Json | undefined
  query: Json = { text: "", section: null, order: "korri" }
  from: Json = { _tag: "Start" }
  count = DEFAULT_COUNT
  focused: string | undefined

  constructor(file: Json) {
    const catalog = file["catalog"] as Json
    this.base = { ...file, catalog: null }
    this.games = catalog["_tag"] === "Ready" ? ((catalog["games"] as Game[] | undefined) ?? []) : []
    this.fixed = catalog["_tag"] === "Ready" ? undefined : catalog
  }

  model(): KorriModel {
    const model: Json = { ...this.base, catalog: this.fixed ?? window(this.games, this.query, this.from, this.count) }
    if (this.focused !== undefined) {
      const actions = (this.base["fixtureGameActions"] as Json | undefined)?.[this.focused]
      model["focusedGame"] = { gameId: this.focused, actions: Array.isArray(actions) ? actions : [] }
    }
    delete model["fixtureGameActions"]
    return model as unknown as KorriModel
  }

  answer(requests: readonly Json[]): boolean {
    let changed = false
    for (const request of requests) {
      if (request["_tag"] === "MoveCatalog") {
        this.query = request["query"] as Json
        this.from = request["from"] as Json
        this.count = typeof request["count"] === "number" ? request["count"] : DEFAULT_COUNT
        changed = true
      }
      if (request["_tag"] === "FocusGame") {
        this.focused = typeof request["gameId"] === "string" ? request["gameId"] : undefined
        changed = true
      }
    }
    return changed
  }
}

/** Every `focused: true` in a view, by its path. */
function focusedPaths(value: unknown, path: string, into: string[]): string[] {
  if (Array.isArray(value)) value.forEach((item, i) => focusedPaths(item, `${path}.${i}`, into))
  else if (value !== null && typeof value === "object") {
    for (const [key, item] of Object.entries(value)) {
      if (key === "focused" && item === true) into.push(path)
      else focusedPaths(item, `${path}.${key}`, into)
    }
  }
  return into
}

/** Pico as the probe runs it: korrid answering, one message at a time. */
export function session(file: Json) {
  const overlaid = (file["presentation"] as Json | undefined)?.["kind"] === "gameplay-overlay"
  const korri = new Korri(file)
  let state: State = init(korri.model(), { [PICO_FONT_KEY]: PICO_DEFAULT_FONT })
  const deliver = (first: unknown) => {
    const pending = [first]
    for (let rounds = 0; pending.length > 0; rounds++) {
      if (rounds >= 16) throw new Error("requests keep causing requests")
      const step = update(state, pending.pop() as Parameters<typeof update>[1], korri.model())
      state = step.state
      if (korri.answer(step.requests as unknown as Json[])) pending.push({ _tag: "KorriPublished" })
    }
  }
  return {
    deliver,
    resize: (width: number, height: number) => deliver({ _tag: "Resized", width, height }),
    press: (...buttons: readonly string[]) => {
      for (const button of buttons) deliver({ _tag: "Pressed", button })
    },
    state: () => state,
    view: () => (overlaid ? (overlay(state, korri.model()) ?? {}) : view(state, korri.model())) as Json,
  }
}

/** Every focused control in a view, by its path. */
export const focusedIn = (shown: unknown): string[] => focusedPaths(shown, "", []).sort()

/** One state of the walk: its presses, and the focus target it shows, if any. */
export interface WalkState {
  readonly path: readonly string[]
  readonly target: string | undefined
}

/** The probe's walk on the logic alone, up to `max` distinct views. */
export function walkStates(file: Json, max: number, window: { readonly width: number; readonly height: number }): WalkState[] {
  const queue: string[][] = [[]]
  const seen = new Set<string>()
  const states: WalkState[] = []
  while (queue.length > 0 && states.length < max) {
    const path = queue.shift() as string[]
    const pico = session(file)
    pico.resize(window.width, window.height)
    pico.deliver({ _tag: "KorriPublished" })
    pico.press(...path)
    const shown = pico.view()
    const key = JSON.stringify(shown)
    if (seen.has(key)) continue
    seen.add(key)
    const focused = focusedIn(shown)
    states.push({ path, target: focused.length === 0 ? undefined : `${pico.state().screen}|${focused.join(",")}` })
    if (path.length < 40) for (const button of PRESSES) queue.push([...path, button])
  }
  return states
}
