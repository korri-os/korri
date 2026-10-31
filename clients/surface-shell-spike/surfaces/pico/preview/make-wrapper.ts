/**
 * Preview only: writes a throwaway wrapper around main.slint with the values
 * logic.js produces, so slint-viewer can draw a state without the shell.
 *
 *   bun preview/make-wrapper.ts <start|afterTwoRights|rights:N> <width> <height> <out.slint>
 *
 * Cover art is decoded to PNG files beside the wrapper (the shell decodes
 * `{ "$image": url }` itself). The wrapper and the PNGs go in preview/gen/,
 * which Git ignores. MODEL=<path> draws another SurfaceModel (preview/pico-fixture-model.ts).
 */
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, join, relative } from "node:path"

const [which = "start", width = "640", height = "480", out = join(import.meta.dir, "gen", "preview.slint")] = process.argv.slice(2)
const here = import.meta.dir
const model = JSON.parse(readFileSync(process.env.MODEL ?? join(here, "../../../fixtures/model.json"), "utf8"))
new Function(readFileSync(join(here, "../logic.js"), "utf8"))()
const surface = (globalThis as { surface?: any }).surface

let state = surface.init(model)
// "afterTwoRights" is two presses of right; "rights:N" is N of them.
const rights = which === "afterTwoRights" ? 2 : which.startsWith("rights:") ? Number(which.slice(7)) : 0
for (let press = 0; press < rights; press += 1) state = surface.update(state, { _tag: "Pressed", action: "right" }, model)
const view = surface.view(state, model)

const outDir = dirname(out)
mkdirSync(join(outDir, "art"), { recursive: true })
const images = new Map<string, string>()
const imageFile = (url: string): string => {
  const known = images.get(url)
  if (known !== undefined) return known
  const file = join(outDir, "art", `${images.size}.png`)
  writeFileSync(file, Buffer.from(url.slice(url.indexOf(",") + 1), "base64"))
  images.set(url, file)
  return file
}

const kebab = (key: string) => key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)
const literal = (value: unknown): string => {
  if (value === null || value === undefined) return ""
  if (Array.isArray(value)) return `[${value.map(literal).join(", ")}]`
  if (typeof value === "object") {
    const record = value as Record<string, unknown>
    if (typeof record.$image === "string") return `@image-url("${relative(outDir, imageFile(record.$image))}")`
    return `{ ${Object.entries(record)
      .filter(([, v]) => v !== null && v !== undefined)
      .map(([k, v]) => `${kebab(k)}: ${literal(v)}`)
      .join(", ")} }`
  }
  return JSON.stringify(value)
}

const props = Object.entries(view as Record<string, unknown>)
  .filter(([, v]) => v !== null && v !== undefined)
  .map(([k, v]) => `    ${kebab(k)}: ${literal(v)};`)
  .join("\n")
const main = relative(outDir, join(here, "../main.slint"))
writeFileSync(out, `import { Surface } from "${main}";

export component Preview inherits Surface {
    width: ${width}px;
    height: ${height}px;
${props}
}
`)
console.log(out)
