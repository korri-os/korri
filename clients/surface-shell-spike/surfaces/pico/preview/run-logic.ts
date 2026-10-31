/**
 * Runs logic.js on fixtures/model.json, as the shell would, without the shell.
 *
 *   bun preview/run-logic.ts            print the view JSON: start, then after right, right
 *   bun preview/run-logic.ts --brief    the same with every data: URL cut short
 */
import { readFileSync } from "node:fs"
import { join } from "node:path"

const here = import.meta.dir
const model = JSON.parse(readFileSync(join(here, "../../../fixtures/model.json"), "utf8"))
new Function(readFileSync(join(here, "../logic.js"), "utf8"))()
const surface = (globalThis as { surface?: any }).surface
if (surface === undefined) throw new Error("logic.js did not define globalThis.surface")

const brief = process.argv.includes("--brief")
const show = (value: unknown) =>
  JSON.stringify(value, (_key, v) => (brief && typeof v === "string" && v.startsWith("data:") ? `${v.slice(0, 30)}…` : v), 2)

let state = surface.init(model)
const start = surface.view(state, model)
state = surface.update(state, { _tag: "Pressed", action: "right" }, model)
state = surface.update(state, { _tag: "Pressed", action: "right" }, model)
const afterTwoRights = surface.view(state, model)
console.log(show({ start, afterTwoRights }))
