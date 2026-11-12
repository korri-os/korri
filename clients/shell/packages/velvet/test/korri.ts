/**
 * A small Korri for the tests: it publishes a scenario's model with the
 * catalog window, `systems` and `focusedGame` computed as korrid does
 * (clients/shell/src/catalog.rs is the same rule in Rust, and the Rust tests
 * use that one), and plays a scenario's steps through Velvet's logic.
 */
import { readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"
import type { Game, SurfaceMessage as Message, SurfaceModel as Model, SurfaceRequest as Request } from "../../../../../contracts/generated/surface"
import "../logic"
import type { State } from "../src/types"

export const PACKAGE = join(import.meta.dir, "..")

/** The sizes Velvet is proven at: the four the design names and the verifier's others. */
export const SIZES = [
  [1920, 1080], [1240, 1080], [640, 480], [480, 800], [320, 240], [1280, 720],
] as const

export interface Scenario {
  readonly name: string
  readonly models: readonly any[]
  readonly steps: readonly string[]
}

export function scenarios(): Scenario[] {
  const dir = join(PACKAGE, "preview/scenarios")
  return readdirSync(dir).filter(name => name.endsWith(".json")).sort()
    .map(name => JSON.parse(readFileSync(join(dir, name), "utf8")) as Scenario)
}

export const scenario = (name: string): Scenario =>
  JSON.parse(readFileSync(join(PACKAGE, "preview/scenarios", `${name}.json`), "utf8")) as Scenario

const lower = (game: Game): string => game.title.toLowerCase()
const number = (value: number | undefined): number => value ?? Number.NEGATIVE_INFINITY

/** The games a query selects, in its order. */
export function select(games: readonly Game[], query: any): Game[] {
  const text = String(query.text ?? "").trim().toLowerCase()
  const selected = games
    .filter(game => text === "" || lower(game).includes(text))
    .filter(game => query.section == null || game.section === query.section)
    .filter(game => query.systemId == null || game.systemId === query.systemId)
  if (query.order === "title") selected.sort((a, b) => (lower(a) < lower(b) ? -1 : lower(a) > lower(b) ? 1 : 0))
  // korrid orders `mostPlayed` by time played (services/korrid/src/surface/catalog.rs).
  if (query.order === "mostPlayed") selected.sort((a, b) => number(b.totalPlaytimeSeconds) - number(a.totalPlaytimeSeconds))
  if (query.order === "recent") selected.sort((a, b) => number(b.lastPlayedAt) - number(a.lastPlayedAt))
  return selected
}

export function systems(games: readonly Game[]) {
  const found: { systemId: string; system: string; total: number }[] = []
  for (const game of games) {
    if (game.systemId === undefined) continue
    const known = found.find(entry => entry.systemId === game.systemId)
    if (known === undefined) found.push({ systemId: game.systemId, system: game.system ?? game.systemId, total: 1 })
    else known.total += 1
  }
  return found.sort((a, b) => a.system.toLowerCase().localeCompare(b.system.toLowerCase()) || a.systemId.localeCompare(b.systemId))
}

export function window(games: readonly Game[], query: any, from: any, count: number) {
  if (games.length === 0) return { _tag: "Empty" }
  const selected = select(games, query)
  const total = selected.length
  const position = (cursor: unknown) => (typeof cursor === "string" && cursor.startsWith("i") ? Number(cursor.slice(1)) : undefined)
  let start = 0
  if (from._tag === "End") start = Math.max(0, total - count)
  if (from._tag === "After") start = Math.min((position(from.cursor) ?? -1) + 1, total)
  if (from._tag === "Before") start = Math.max(0, Math.min(position(from.cursor) ?? 0, total) - count)
  if (from._tag === "Around") {
    const at = selected.findIndex(game => game.id === from.gameId)
    start = at < 0 ? 0 : Math.min(Math.max(0, at - Math.floor(count / 2)), Math.max(0, total - count))
  }
  const end = Math.min(start + count, total)
  return {
    _tag: "Ready", total, window: selected.slice(start, end), firstIndex: start, query,
    ...(start > 0 ? { before: `i${start}` } : {}), ...(end < total ? { after: `i${end - 1}` } : {}),
  }
}

/** One play of Velvet against the small Korri. */
export class Play {
  state!: State
  model!: Model
  /** Every request Velvet made, in order. */
  readonly requests: Request[] = []
  /** The most MoveCatalog requests one `update` returned, and how many updates ran. */
  mostMovesInOneUpdate = 0
  updates = 0
  private base: any
  private games: Game[] | undefined
  private query: any = { text: "", section: null, order: "korri" }
  private from: any = { _tag: "Start" }
  private count = 50
  private focused: string | undefined
  private actions: unknown

  constructor(model: any, width = 1280, height = 720, stored: Record<string, string> = {}) {
    this.take(model)
    this.publish()
    // As the shell starts a surface: the model, `init`, then `Resized` before the first view.
    // (No `KorriPublished` here: the shell sends one with each model Korri pushes, and to
    // Velvet a push that shows nothing new means Korri changed something out of sight.)
    this.state = globalThis.surface.init(this.model, stored)
    this.deliver({ _tag: "Resized", width, height })
  }

  private take(model: any): void {
    this.games = Array.isArray(model.catalog?.games) ? model.catalog.games : undefined
    this.actions = model.focusedGame?.actions ?? []
    const { focusedGame: _, ...base } = model
    this.base = base
  }

  private publish(): void {
    const model = { ...this.base }
    if (this.games !== undefined) {
      model.catalog = window(this.games, this.query, this.from, this.count)
      model.systems = systems(this.games)
    }
    if (this.focused !== undefined) model.focusedGame = { gameId: this.focused, actions: this.actions }
    // The shell hands Velvet each model as JSON: every publication is new objects all the way
    // down, and nothing may lean on a game being the same object as the one published before.
    this.model = JSON.parse(JSON.stringify(model)) as Model
  }

  /** Korri publishes another model (a scenario's `model N`). */
  swap(model: any): void {
    this.take(model)
    this.publish()
    this.deliver({ _tag: "KorriPublished" })
  }

  deliver(message: Message): Request[] {
    const made: Request[] = []
    const queue: Message[] = [message]
    while (queue.length > 0) {
      const result = globalThis.surface.update(this.state, queue.pop()!, this.model)
      this.state = result.state
      this.updates += 1
      this.mostMovesInOneUpdate = Math.max(this.mostMovesInOneUpdate, (result.requests as any[]).filter(request => request._tag === "MoveCatalog").length)
      let changed = false
      for (const request of result.requests as any[]) {
        made.push(request)
        this.requests.push(request)
        if (request._tag === "MoveCatalog") {
          this.query = request.query
          this.from = request.from
          this.count = request.count ?? 50
          changed = true
        }
        if (request._tag === "FocusGame") {
          this.focused = request.gameId ?? undefined
          changed = true
        }
      }
      if (changed) {
        this.publish()
        queue.push({ _tag: "KorriPublished" })
      }
    }
    return made
  }

  press(...buttons: string[]): Request[] {
    return buttons.flatMap(button => this.deliver({ _tag: "Pressed", button } as Message))
  }

  step(step: string, models: readonly any[]): void {
    if (step.startsWith("model ")) this.swap(models[Number(step.slice(6))])
    else if (step.startsWith("wait")) this.deliver({ _tag: "Tick", idleMs: Math.round(Number(step.slice(5) || "1") * 1000) })
    else this.press(step)
  }

  /** How many MoveCatalog requests Velvet has made. */
  get moves(): number {
    return this.requests.filter((request: any) => request._tag === "MoveCatalog").length
  }

  get view(): any {
    return this.overlay ? globalThis.surface.overlay(this.state, this.model) : globalThis.surface.view(this.state, this.model)
  }

  get overlay(): boolean {
    return (this.model as any).presentation?.kind === "gameplay-overlay"
  }
}

/** Play a scenario to its end at a size. */
export function play(scene: Scenario, width = 1280, height = 720, each?: (play: Play, step: string) => void): Play {
  const run = new Play(scene.models[0], width, height)
  each?.(run, "start")
  for (const step of scene.steps) {
    run.step(step, scene.models)
    each?.(run, step)
  }
  return run
}

/** Every entry of a view that is a focus target (it carries `focused`). */
export function targets(view: unknown, out: any[] = []): any[] {
  if (Array.isArray(view)) for (const item of view) targets(item, out)
  else if (view !== null && typeof view === "object") {
    const entry = view as Record<string, unknown>
    if (typeof entry.focused === "boolean" && typeof entry.focusRegion === "string") out.push(entry)
    for (const key of Object.keys(entry)) targets(entry[key], out)
  }
  return out
}

/** Every picture a view names, apart from the empty one. */
export function pictures(view: unknown, out = new Set<string>()): Set<string> {
  if (Array.isArray(view)) for (const item of view) pictures(item, out)
  else if (view !== null && typeof view === "object") {
    const entry = view as Record<string, unknown>
    if (typeof entry.$image === "string") {
      if (!entry.$image.startsWith("data:")) out.add(entry.$image)
    } else for (const key of Object.keys(entry)) pictures(entry[key], out)
  }
  return out
}
