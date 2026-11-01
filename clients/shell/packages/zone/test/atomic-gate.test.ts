/**
 * Atomic: one component per file, layers point down, shapes read no state.
 *
 * Only main.slint is the binding: it alone inherits Window for the shell
 * (Surface, Overlay), takes the view's properties, and writes the Tokens
 * global's inputs. Every component below it gets plain properties and may
 * read Tokens (the window's size, the look, the clock), never write it.
 * A file imports its own layer or lower, and the foundation (Tokens, the
 * view's struct types); a preview also imports the preview frame.
 */
import { describe, expect, test } from "bun:test"
import { componentsIn, FOUNDATION, importsOf, isPreview, layerOf, LAYERS, read, slintFiles, stripComments } from "./slint-source"

const FRAME = "ui/preview-frame.slint"
const files = slintFiles()
const components = files.filter(file => layerOf(file) !== undefined && !isPreview(file))

describe("atomic", () => {
  test("exactly one component per component file", () => {
    const wrong = components.filter(file => componentsIn(read(file)).length !== 1)
    expect(wrong.map(file => `${file}: ${componentsIn(read(file)).join(", ")}`)).toEqual([])
  })

  test("the foundation declares no component", () => {
    expect(FOUNDATION.filter(file => componentsIn(read(file)).length > 0)).toEqual([])
  })

  test("imports point down: a file imports its own layer or lower, and the foundation", () => {
    const wrong: string[] = []
    for (const file of files.filter(f => layerOf(f) !== undefined)) {
      const rank = LAYERS.indexOf(layerOf(file)!)
      for (const target of importsOf(file, read(file))) {
        if ((FOUNDATION as readonly string[]).includes(target) || target.startsWith("fonts/") || target.startsWith("art/")) continue
        if (isPreview(file) && (target === FRAME || target === file.replace(/\.[a-z]+\.preview\.slint$/, ".slint"))) continue
        const layer = layerOf(target)
        if (layer === undefined || LAYERS.indexOf(layer) > rank) wrong.push(`${file} → ${target}`)
      }
    }
    expect(wrong).toEqual([])
  })

  test("only main.slint (and the preview frame) inherits Window", () => {
    const wrong = files.filter(file => file !== "main.slint" && file !== FRAME && !isPreview(file))
      .filter(file => /inherits\s+Window\b/.test(stripComments(read(file))))
    expect(wrong).toEqual([])
  })

  test("nothing below main.slint writes the Tokens global", () => {
    const wrong = files.filter(file => file !== "main.slint" && file !== FRAME && !isPreview(file))
      .filter(file => /Tokens\.[\w-]+\s*=[^=]/.test(stripComments(read(file))))
    expect(wrong).toEqual([])
  })

  test("only main.slint knows the view: no component imports main.slint or declares the shell's window properties", () => {
    const wrong = files.filter(file => file !== "main.slint")
      .filter(file => importsOf(file, read(file)).includes("main.slint") || /in\s+property\s+<int>\s+presses\b/.test(stripComments(read(file))))
    expect(wrong).toEqual([])
  })
})
