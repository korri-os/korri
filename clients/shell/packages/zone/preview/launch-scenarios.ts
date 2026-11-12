/**
 * LAUNCH SETTINGS' preview scenarios (ADR 0066): the hand-written one-list
 * models of test/fixtures/launch-settings.ts, each laid over the Korri model
 * the other scenarios share (preview/scenarios/options.json), with the
 * presses to the state it shows.
 *
 *     cd clients/shell && bun packages/zone/preview/launch-scenarios.ts
 *
 * It writes preview/scenarios/launch*.json, which preview/probe.sh, the
 * shell's tests (tests/zone_package.rs) and test/walk.ts replay, and the
 * page's preview states (pages/ZoneLaunchSettings.page.preview.slint): each
 * the view logic.ts makes of its scenario at 640x480, so a preview state
 * and its scenario never disagree.
 */
import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import type { SurfaceMessage, SurfaceModel } from "../../../../../contracts/generated/surface"
import { init, update, view } from "../logic"
import { launchFixtures, type LaunchFixture } from "../test/fixtures/launch-settings"

const scenarios = join(import.meta.dir, "scenarios")
const base = JSON.parse(readFileSync(join(scenarios, "options.json"), "utf8")).models[0] as Record<string, unknown>

/** The scenario's name, its fixture, and the presses to the state it shows. */
const LAUNCH: readonly (readonly [string, LaunchFixture, readonly string[]])[] = [
  ["launch", "local", ["down"]],
  ["launch-changes", "changes", []],
  ["launch-streamed", "streamed", ["down", "down", "down", "down", "down"]],
  ["launch-unreachable", "unreachable", ["down"]],
  ["launch-long", "long", ["down", "down", "down", "down", "down"]],
  ["launch-korri", "korriOnly", []],
  ["launch-empty", "empty", []],
  ["launch-loading", "loading", []],
  ["launch-conflict", "conflict", []],
  ["launch-error", "error", []],
]

// ---- The page's preview states ----

/** A view value as a .slint literal: kebab-case fields, and no field at its default (a row's `bars: -1` aside). */
function literal(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(literal).join(", ")}]`
  if (value !== null && typeof value === "object") {
    const fields = Object.entries(value as Record<string, unknown>)
      .filter(([key, inner]) => !["focusRegion", "focusActive", "line", "holdsLine"].includes(key) && (key === "bars" ? inner !== 0 : !(inner === false || inner === "" || inner === 0 || (Array.isArray(inner) && inner.length === 0))))
      .map(([key, inner]) => `${key.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`)}: ${literal(inner)}`)
    return `{ ${fields.join(", ")} }`
  }
  return typeof value === "string" ? JSON.stringify(value) : String(value)
}

/** The view of a model after its presses, at 640x480, as the shell would have it. */
function shown(scenario: Record<string, unknown>, steps: readonly string[]): Record<string, unknown> {
  // A scenario's catalog lists its games; korrid publishes them as a window (test/walk.ts does the same).
  const games = (scenario.catalog as { games?: unknown[] } | undefined)?.games ?? []
  const model = { ...scenario, catalog: { _tag: "Ready", total: games.length, window: games, firstIndex: 0, query: { text: "", order: "korri" } } } as unknown as SurfaceModel
  let state = init(model, {})
  const send = (message: SurfaceMessage) => {
    state = update(state, message, model).state
  }
  send({ _tag: "Resized", width: 640, height: 480 })
  send({ _tag: "KorriPublished" })
  for (const step of steps) send({ _tag: "Pressed", button: step } as SurfaceMessage)
  return view(state, model) as unknown as Record<string, unknown>
}

const capital = (words: string) => words.charAt(0).toUpperCase() + words.slice(1)
const states: string[] = []

for (const [name, fixture, steps] of LAUNCH) {
  const model = { ...base, launchSettings: launchFixtures[fixture] }
  writeFileSync(join(scenarios, `${name}.json`), `${JSON.stringify({ name, ref: ["ZoneLaunchSettings", capital(fixture)], models: [model], steps }, null, 1)}\n`)
  const drawn = shown(model, steps)
  states.push([
    `export component ${capital(fixture)} inherits ZonePreview {`,
    `    ground: "options";`,
    `    ZoneLaunchSettings { x: 0; y: 0; width: parent.width; height: parent.height; hints: ${literal(drawn.hints)}; foot: ${literal(drawn.foot)}; head: ${literal(drawn.head)}; rows: ${literal(drawn.rows)}; keep: ${literal(drawn.keep)}; }`,
    `}`,
    ``,
  ].join("\n"))
}

const preview = [
  `// ZoneLaunchSettings's preview states: the view logic.ts makes of each LAUNCH SETTINGS scenario`,
  `// (preview/scenarios/launch*.json, the one-list models of test/fixtures/launch-settings.ts) at 640x480.`,
  `// Written by preview/launch-scenarios.ts.`,
  ``,
  `import { Tokens } from "../ui/tokens.slint";`,
  `import { ZonePreview } from "../ui/preview-frame.slint";`,
  `import { ZoneLaunchSettings } from "./ZoneLaunchSettings.slint";`,
  ``,
  ...states,
].join("\n")
writeFileSync(join(import.meta.dir, "..", "pages", "ZoneLaunchSettings.page.preview.slint"), preview)
console.log(`${LAUNCH.length} launch scenarios in ${scenarios}, and the page's preview states`)
