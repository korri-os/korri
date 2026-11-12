/**
 * Launch settings' preview states (ADR 0066): the view logic.ts makes of each
 * hand-written one-list model (test/fixtures/launch-settings.ts) at 640x480,
 * written as pages/PicoLaunchSettings.page.preview.slint, so a preview state
 * and the model it shows never disagree.
 *
 *     cd clients/shell/packages/pico && bun preview/launch-previews.ts
 */
import { writeFileSync } from "node:fs"
import { join } from "node:path"
import type { KorriModel, SurfaceMessage } from "../src/model"
import { init, update, view } from "../src/native/program"
import { fixtureModel } from "../test/fixtures/fixture-host"
import { launchFixtures, type LaunchFixture } from "../test/fixtures/launch-settings"

/** The state's name, its fixture, and the presses to what it shows. */
const STATES: readonly (readonly [string, LaunchFixture, readonly string[]])[] = [
  ["Local", "local", []],
  ["Changes", "changes", []],
  ["KeepFocused", "changes", ["down", "down", "down", "down", "down", "down", "down", "down", "down"]],
  ["Streamed", "streamed", ["down", "down", "down", "down", "down"]],
  ["Unreachable", "unreachable", []],
  ["Long", "long", ["down", "down", "down", "down", "down"]],
  ["KorriOnly", "korriOnly", []],
  ["Empty", "empty", []],
  ["Loading", "loading", []],
  ["Conflict", "conflict", []],
  ["Error", "error", []],
]

/** A view value as a .slint literal: kebab-case fields, without the focus plan's own keys. */
function literal(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(literal).join(", ")}]`
  if (value !== null && typeof value === "object") {
    const fields = Object.entries(value as Record<string, unknown>)
      .filter(([key]) => key !== "focusRegion" && key !== "focusActive")
      .map(([key, inner]) => `${key.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`)}: ${literal(inner)}`)
    return `{ ${fields.join(", ")} }`
  }
  return typeof value === "string" ? JSON.stringify(value) : String(value)
}

/** The view of a launch model after its presses, at 640x480, as the shell would have it. */
function shown(fixture: LaunchFixture, steps: readonly string[]): Record<string, unknown> {
  const games = fixtureModel.catalog._tag === "Ready" ? [...fixtureModel.catalog.games] : []
  const model = {
    ...fixtureModel,
    catalog: { _tag: "Ready", total: games.length, window: games, firstIndex: 0, query: { text: "", order: "korri" } },
    launchSettings: launchFixtures[fixture],
  } as unknown as KorriModel
  let state = init(model, {})
  const send = (message: SurfaceMessage) => {
    state = update(state, message, model).state
  }
  send({ _tag: "Resized", width: 640, height: 480 })
  send({ _tag: "KorriPublished" })
  for (const step of steps) send({ _tag: "Pressed", button: step } as SurfaceMessage)
  return view(state, model) as unknown as Record<string, unknown>
}

const states = STATES.map(([name, fixture, steps]) => {
  const drawn = shown(fixture, steps)
  return [
    `export component ${name} inherits Window {`,
    `    background: P8.bg;`,
    `    default-font-family: Tokens.family;`,
    `    init => { Tokens.fit(self.width, self.height); }`,
    `    changed width => { Tokens.fit(self.width, self.height); }`,
    `    changed height => { Tokens.fit(self.width, self.height); }`,
    `    PicoLaunchSettings {`,
    `        x: 0;`,
    `        y: 0;`,
    `        width: parent.width;`,
    `        height: parent.height;`,
    `        mode: ${literal(drawn.mode)};`,
    `        frame: ${literal(drawn.frame)};`,
    `        panel: ${literal(drawn.panel)};`,
    `        card: ${literal(drawn.card)};`,
    `    }`,
    `}`,
    ``,
  ].join("\n")
})

const preview = [
  `// Preview states of PicoLaunchSettings: the view logic.ts makes of each one-list model`,
  `// (test/fixtures/launch-settings.ts) at 640x480. Written by preview/launch-previews.ts.`,
  `// Render one: slint-viewer --component STATE --screenshot out.png PicoLaunchSettings.page.preview.slint`,
  `import { P8 } from "../ui/palette.slint";`,
  `import { Tokens } from "../ui/tokens.slint";`,
  `import { PicoLaunchSettings } from "PicoLaunchSettings.slint";`,
  ``,
  ``,
  ...states,
].join("\n")
writeFileSync(join(import.meta.dir, "..", "pages", "PicoLaunchSettings.page.preview.slint"), preview)
console.log(`${STATES.length} preview states of PicoLaunchSettings`)
