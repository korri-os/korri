/**
 * Zone's scenarios (preview/scenarios) played through logic.ts as the shell
 * plays them, with a mini korrid that answers MoveCatalog and FocusGame the
 * way tests/zone_package.rs's does: the catalog window holds every game.
 */
import { readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"
import type { SurfaceButton, SurfaceModel } from "../../../../../contracts/generated/surface"
import { init, update, view, overlay, type State } from "../logic"

export const SCENARIOS = join(import.meta.dir, "..", "preview", "scenarios")

export const SIZES = [
  [320, 240],
  [640, 480],
  [480, 800],
  [1280, 720],
  [1920, 1080],
] as const

interface Scenario {
  readonly models: readonly Record<string, unknown>[]
  readonly steps: readonly string[]
}

export const scenarioNames = (): string[] =>
  readdirSync(SCENARIOS).filter(name => name.endsWith(".json")).map(name => name.slice(0, -5)).sort()

/** The scenario's model as korrid publishes it. */
function published(raw: Record<string, unknown>, focused: string | undefined, actions: unknown, text: string): SurfaceModel {
  const model: Record<string, unknown> = { ...raw }
  const catalog = raw.catalog as { _tag: string; games?: { title: string }[] }
  if (catalog?.games !== undefined) {
    const needle = text.trim().toLocaleLowerCase()
    const games = catalog.games.filter(game => needle === "" || game.title.toLocaleLowerCase().includes(needle))
    model.catalog = { _tag: "Ready", total: games.length, window: games, firstIndex: 0, query: { text, order: "korri" } }
  }
  delete model.focusedGame
  if (focused !== undefined) model.focusedGame = { gameId: focused, actions }
  return model as unknown as SurfaceModel
}

export class Walk {
  state: State
  model: SurfaceModel
  private raw: Record<string, unknown>
  private focused: string | undefined
  private actions: unknown
  private text = ""
  readonly overlay: boolean

  constructor(readonly name: string, width: number, height: number) {
    const scenario = JSON.parse(readFileSync(join(SCENARIOS, `${name}.json`), "utf8")) as Scenario
    this.raw = scenario.models[0]!
    this.actions = (this.raw.focusedGame as { actions?: unknown } | undefined)?.actions ?? []
    this.model = published(this.raw, undefined, this.actions, "")
    this.overlay = (this.raw.presentation as { kind?: string } | undefined)?.kind === "gameplay-overlay"
    this.state = init(this.model, {})
    this.deliver({ _tag: "Resized", width, height })
    this.deliver({ _tag: "KorriPublished" })
    for (const step of scenario.steps) {
      if (step.startsWith("model ")) {
        this.raw = scenario.models[Number(step.slice(6))]!
        this.model = published(this.raw, this.focused, this.actions, this.text)
        this.deliver({ _tag: "KorriPublished" })
      } else if (!step.startsWith("wait")) this.press(step as SurfaceButton)
    }
  }

  deliver(message: Parameters<typeof update>[1]): void {
    const queue = [message]
    while (queue.length > 0) {
      const step = update(this.state, queue.shift()!, this.model)
      this.state = step.state
      let changed = false
      for (const request of step.requests) {
        if (request._tag === "MoveCatalog") {
          this.text = request.query.text
          changed = true
        }
        if (request._tag === "FocusGame") {
          this.focused = request.gameId
          changed = true
        }
      }
      if (changed) {
        this.model = published(this.raw, this.focused, this.actions, this.text)
        queue.push({ _tag: "KorriPublished" })
      }
    }
  }

  press(button: SurfaceButton): void {
    this.deliver({ _tag: "Pressed", button })
  }

  view(): Record<string, unknown> {
    return (this.overlay ? overlay(this.state, this.model) : view(this.state, this.model)) as Record<string, unknown>
  }
}
