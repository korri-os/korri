/**
 * Decomposition: Velvet in parts, each drawn alone.
 *
 *  - Every component (ui/atoms, ui/molecules, ui/organisms, pages) has its
 *    preview file beside it, `<Name>.<layer>.preview.slint`: named states
 *    that inherit VelvetPreview and draw the component with fixture data.
 *    preview/previews.ts writes them from Velvet's scenarios; the shell's
 *    own interpreter draws every state at five sizes
 *    (clients/shell/tests/velvet_package.rs).
 *  - No preview stands without its component, and no component stands
 *    unused: each is drawn by another component or by main.slint.
 *  - The plan and the views agree: `Screen` in ui/types.slint has exactly
 *    the plan's fields (src/plan.ts), and the Tokens' default plan names
 *    them all.
 *  - Every struct the view sends is declared once, in ui/types.slint.
 */
import { describe, expect, test } from "bun:test"
import { existsSync } from "node:fs"
import { join } from "node:path"
import { plan } from "../src/plan"
import { componentsIn, importsOf, isPreview, LAYER_SUFFIX, layerOf, PACKAGE, read, slintFiles, stripComments } from "./slint-source"

const files = slintFiles()
const components = files.filter(file => layerOf(file) !== undefined && !isPreview(file))
const previews = files.filter(isPreview)
const previewOf = (file: string) => file.replace(/\.slint$/, `.${LAYER_SUFFIX[layerOf(file)!]}.preview.slint`)
const kebab = (name: string) => name.replace(/[A-Z]/g, c => `-${c.toLowerCase()}`)

describe("decomposition", () => {
  test("the layers hold what the README lists", () => {
    const count = (layer: string) => components.filter(file => layerOf(file) === layer).length
    expect(count("atoms")).toBeGreaterThanOrEqual(15)
    expect(count("molecules")).toBeGreaterThanOrEqual(8)
    expect(count("organisms")).toBeGreaterThanOrEqual(10)
    expect(count("pages")).toBe(6)
  })

  test("every component has its preview beside it", () => {
    expect(components.filter(file => !existsSync(join(PACKAGE, previewOf(file))))).toEqual([])
  })

  test("every preview draws its own component in at least one named state", () => {
    const wrong: string[] = []
    for (const file of components) {
      const preview = previewOf(file)
      if (!existsSync(join(PACKAGE, preview))) continue
      const text = stripComments(read(preview))
      const name = componentsIn(read(file))[0]!
      const states = [...text.matchAll(/export component (\w+) inherits VelvetPreview\b/g)].map(match => match[1]!)
      if (states.length === 0) wrong.push(`${preview}: no state`)
      if (new Set(states).size !== states.length) wrong.push(`${preview}: a state is named twice`)
      if (!new RegExp(`(?<![\\w-])${name}\\s*\\{`).test(text)) wrong.push(`${preview}: does not draw ${name}`)
      if (componentsIn(text).length !== states.length) wrong.push(`${preview}: a component that is no preview state`)
    }
    expect(wrong).toEqual([])
  })

  test("no preview stands without its component", () => {
    expect(previews.filter(file => !existsSync(join(PACKAGE, file.replace(/\.[a-z]+\.preview\.slint$/, ".slint"))))).toEqual([])
  })

  test("no component stands unused", () => {
    const used = new Set(files.filter(file => !isPreview(file)).flatMap(file => importsOf(file, read(file))))
    expect(components.filter(file => !used.has(file))).toEqual([])
  })

  test("the previews hold at least 150 states", () => {
    const states = previews.reduce((sum, file) => sum + componentsIn(read(file)).length, 0)
    expect(states).toBeGreaterThanOrEqual(150)
  })
})

describe("the plan and the views agree", () => {
  const types = stripComments(read("ui/types.slint"))
  const screen = /export struct Screen\s*\{([^}]*)\}/.exec(types)![1]!
  const fields = [...screen.matchAll(/([\w-]+)\s*:\s*(\w+)/g)].map(match => [match[1]!, match[2]!] as const)
  const planned = plan(1280, 720) as unknown as Record<string, unknown>

  test("Screen has exactly the plan's fields", () => {
    expect(fields.map(([name]) => name).sort()).toEqual(Object.keys(planned).map(kebab).sort())
  })

  test("each field has the plan's type", () => {
    for (const [name, type] of fields) {
      const key = Object.keys(planned).find(candidate => kebab(candidate) === name)!
      // A number is a float (a length in px) or an int (a count); a flag is a bool.
      if (typeof planned[key] === "boolean") expect(type, name).toBe("bool")
      else expect(["float", "int"], name).toContain(type)
      if (type === "int") expect(Number.isInteger(planned[key]), name).toBe(true)
    }
  })

  test("the Tokens' default plan names every field", () => {
    const tokens = stripComments(read("ui/tokens.slint"))
    const start = tokens.indexOf("in-out property <Screen> screen:")
    const literal = tokens.slice(start, tokens.indexOf("};", start))
    for (const [name] of fields) expect(new RegExp(`(?<![\\w-])${name}:`).test(literal), name).toBe(true)
  })

  test("every struct is declared once, in ui/types.slint", () => {
    const declared = [...types.matchAll(/export struct (\w+)/g)].map(match => match[1]!)
    expect(new Set(declared).size).toBe(declared.length)
    const elsewhere = files.filter(file => file !== "ui/types.slint" && /\bstruct\s+\w+\s*\{/.test(stripComments(read(file))))
    expect(elsewhere).toEqual([])
  })
})
