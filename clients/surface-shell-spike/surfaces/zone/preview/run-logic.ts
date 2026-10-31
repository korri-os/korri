/**
 * THROWAWAY SPIKE proof: runs the bundled logic.js as the shell does (one
 * fresh context, nothing in it but the script), on fixtures/model.json, and
 * prints the view JSON for the start state and after each of two presses of
 * right. Data URLs are shortened in the printout.
 *
 * With --write it also writes, for the preview renders only:
 *   preview/art/<n>.png       each card's cover, decoded from its data URL
 *   preview/view-<step>.json  the view with each $image replaced by that file
 *
 *   bun preview/run-logic.ts [--write]      (from surfaces/zone/)
 */
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { runInNewContext } from "node:vm"

const here = dirname(new URL(import.meta.url).pathname)
const pkg = join(here, "..")
const model = JSON.parse(readFileSync(join(pkg, "../../fixtures/model.json"), "utf8"))
const code = readFileSync(join(pkg, "logic.js"), "utf8")

// A bare context: no console, no timers, no Date beyond the language's own.
const context: Record<string, unknown> = {}
runInNewContext(code, context)
const surface = (context as { surface?: Record<string, (...args: unknown[]) => unknown> }).surface
if (surface === undefined) throw new Error("logic.js defined no globalThis.surface")
const extra = Object.keys(context).filter(key => key !== "surface")
if (extra.length > 0) throw new Error(`logic.js left globals: ${extra.join(", ")}`)

const right = { _tag: "Pressed", action: "right" }
const steps: [string, unknown][] = []
let state = surface.init!(model)
steps.push(["start", surface.view!(state, model)])
state = surface.update!(state, right, model)
steps.push(["right-1", surface.view!(state, model)])
state = surface.update!(state, right, model)
steps.push(["right-2", surface.view!(state, model)])

const short = (_key: string, value: unknown) =>
  typeof value === "string" && value.startsWith("data:") ? `${value.slice(0, 32)}…(${value.length} chars)` : value
for (const [name, view] of steps) {
  console.log(`== ${name}`)
  console.log(JSON.stringify(view, short, 2))
}

// The cursor's other places, for the focus renders only (not printed).
const press = (action: string) => ({ _tag: "Pressed", action })
const from = (actions: readonly string[]) => {
  let at = surface.init!(model)
  for (const action of actions) at = surface.update!(at, press(action), model)
  return surface.view!(at, model)
}
const focusSteps: [string, unknown][] = [
  ["up", from(["up"])],
  ["up-right", from(["up", "right"])],
  ["menu-menu", from(["menu", "menu"])],
  ["down", from(["down"])],
  ["right-2-down-right", from(["right", "right", "down", "right"])],
]

if (process.argv.includes("--write")) {
  steps.push(...focusSteps)
  const art = join(here, "art")
  mkdirSync(art, { recursive: true })
  const files = new Map<string, string>()
  const toFile = (url: string): string => {
    const known = files.get(url)
    if (known !== undefined) return known
    const match = /^data:image\/png;base64,(.*)$/.exec(url)
    if (match === null) throw new Error("only PNG data URLs are decoded here")
    const name = `art/${files.size}.png`
    writeFileSync(join(here, name), Buffer.from(match[1]!, "base64"))
    files.set(url, name)
    return name
  }
  for (const [name, view] of steps) {
    const swapped = JSON.parse(JSON.stringify(view), (_key, value) =>
      value !== null && typeof value === "object" && typeof value.$image === "string" ? { $image: toFile(value.$image) } : value)
    writeFileSync(join(here, `view-${name}.json`), `${JSON.stringify(swapped, null, 2)}\n`)
  }
}
