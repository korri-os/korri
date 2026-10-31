/**
 * THROWAWAY BENCHMARK. Runs Zone's real, unchanged Elm `update`
 * (surfaces/zone/src/state/surface.ts) on Zone's own fixtures, so the cost of
 * running surface logic as TypeScript can be measured inside QuickJS on the
 * device. Bundled by bun into one IIFE script; the Rust host supplies
 * `__now_us()` (a monotonic clock in microseconds) and calls `runBench`.
 */
import { update, zoneShown, zoneSurfaceStart, type ZoneSurfaceMessage, type ZoneSurfaceState } from "../../../surfaces/zone/src/state/surface"
import { fixtureReadyModel, gamesModel, fixtureLibrary } from "../../../surfaces/zone/src/fixtures/zone-games"
import type { SurfaceGame, SurfaceModel } from "../../../contracts/surface/korri-surface"

declare const __now_us: () => number

interface Result {
  readonly name: string
  readonly n: number
  readonly p50: number
  readonly p95: number
  readonly p99: number
  readonly max: number
  readonly mean: number
}

function summarise(name: string, samples: number[]): Result {
  const sorted = [...samples].sort((a, b) => a - b)
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))]!
  const mean = samples.reduce((a, b) => a + b, 0) / samples.length
  return { name, n: samples.length, p50: at(0.5), p95: at(0.95), p99: at(0.99), max: sorted[sorted.length - 1]!, mean }
}

/** A library of `count` games, each a copy of a fixture game with its own id and title. */
function library(count: number): readonly SurfaceGame[] {
  return Array.from({ length: count }, (_, index) => {
    const base = fixtureLibrary[index % fixtureLibrary.length]!
    return { ...base, id: `${base.id}-${index}`, title: `${base.title} ${index}` }
  })
}

/** One input, as Zone receives it today: the band's focus moves, A, B, the menu button. */
function navigationScript(cards: number): ZoneSurfaceMessage[] {
  const script: ZoneSurfaceMessage[] = []
  for (let index = 0; index < cards; index += 1) script.push({ _tag: "Games", message: { _tag: "FocusedCard", index } })
  script.push({ _tag: "Games", message: { _tag: "ChoseCard", index: Math.floor(cards / 2) } })
  script.push({ _tag: "Games", message: { _tag: "FocusedAction", index: 0 } })
  script.push({ _tag: "PressedBack" })
  script.push({ _tag: "PressedMenu" })
  script.push({ _tag: "PressedBack" })
  return script
}

/** Time `steps` inputs through update, each followed by what the shell would read back. */
function timeInputs(name: string, korri: SurfaceModel, steps: number, marshal: boolean): Result {
  let state: ZoneSurfaceState = zoneSurfaceStart(korri, "modern")
  const games = korri.catalog._tag === "Ready" ? korri.catalog.games.length : 1
  const script = navigationScript(Math.min(games, 40))
  const samples: number[] = []
  let sink = 0
  for (let step = 0; step < steps; step += 1) {
    const message = script[step % script.length]!
    const start = __now_us()
    const next = update(state, message, korri)
    state = next.model
    const shown = zoneShown(state, korri)
    if (marshal) sink += JSON.stringify({ shown, nav: state.nav, requests: next.requests }).length
    else sink += shown.length + next.requests.length
    samples.push(__now_us() - start)
  }
  if (sink < 0) throw new Error("unreachable")
  return summarise(name, samples)
}

/** Time Korri publishing a new model: every model is settled against it, as on each korrid update. */
function timePublish(name: string, korri: SurfaceModel, steps: number): Result {
  let state: ZoneSurfaceState = zoneSurfaceStart(korri, "modern")
  const samples: number[] = []
  for (let step = 0; step < steps; step += 1) {
    const start = __now_us()
    state = update(state, { _tag: "KorriPublished" }, korri).model
    samples.push(__now_us() - start)
  }
  return summarise(name, samples)
}

function timeStart(name: string, korri: SurfaceModel, steps: number): Result {
  const samples: number[] = []
  for (let step = 0; step < steps; step += 1) {
    const start = __now_us()
    zoneSurfaceStart(korri, "modern")
    samples.push(__now_us() - start)
  }
  return summarise(name, samples)
}

/** The model as the shell would hand it in: JSON text parsed in the engine. */
function timeModelIn(name: string, korri: SurfaceModel, steps: number): Result {
  const text = JSON.stringify(korri)
  const samples: number[] = []
  let sink = 0
  for (let step = 0; step < steps; step += 1) {
    const start = __now_us()
    const parsed = JSON.parse(text) as SurfaceModel
    sink += parsed.catalog._tag.length
    samples.push(__now_us() - start)
  }
  if (sink < 0) throw new Error("unreachable")
  return summarise(name, samples)
}

;(globalThis as Record<string, unknown>).runBench = (steps: number): string => {
  const big = gamesModel({ _tag: "Ready", games: library(500) })
  const results: Result[] = [
    timeStart("start (fixture, 7 games)", fixtureReadyModel, 50),
    timeInputs("input (fixture, 7 games)", fixtureReadyModel, steps, false),
    timeInputs("input + JSON out (fixture)", fixtureReadyModel, steps, true),
    timeInputs("input (500 games)", big, steps, false),
    timePublish("KorriPublished (fixture)", fixtureReadyModel, Math.floor(steps / 4)),
    timePublish("KorriPublished (500 games)", big, Math.floor(steps / 4)),
    timeModelIn("model JSON.parse (fixture)", fixtureReadyModel, Math.floor(steps / 4)),
    timeModelIn("model JSON.parse (500 games)", big, Math.floor(steps / 4)),
  ]
  const bigModelText = JSON.stringify(big)
  return JSON.stringify({ results, bigModelText, modelBytes: { fixture: JSON.stringify(fixtureReadyModel).length, big: bigModelText.length } })
}
