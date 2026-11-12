/**
 * Touch reaches the root (ADR 0055).
 *
 *  - Every component that declares the `activated` (or `dragged`) callback
 *    has it forwarded on every instance, up to main.slint's two roots. An
 *    instance that drops it leaves its targets drawn but untappable. The
 *    one exception is an instance written `inert: true`: a picture, with no
 *    target to tap.
 *  - Every element that draws a focus target (`accessible-item-selectable`)
 *    stands in a component that can report a tap: it holds a VelvetTap, or
 *    its parent lays one over it (the set's marks).
 *  - VelvetTap is the one place a press becomes an activation: a drag never
 *    activates, a cancel completes nothing, and a lift the area did not see
 *    begin completes nothing.
 *  - A touch screen has no Back button: the roots hold the way back
 *    (VelvetBack, and a tap beside a sheet or over the keyboard's field).
 */
import { describe, expect, test } from "bun:test"
import { componentsIn, isPreview, read, slintFiles, stripComments } from "./slint-source"

const files = slintFiles().filter(file => !isPreview(file) && file !== "ui/preview-frame.slint")
const source = (file: string) => stripComments(read(file))

/** The components that declare `callback name(`. */
function declaring(name: string): Set<string> {
  const found = new Set<string>()
  for (const file of files) {
    if (file === "main.slint") continue
    if (new RegExp(`callback\\s+${name}\\s*\\(`).test(source(file))) for (const component of componentsIn(read(file))) found.add(component)
  }
  return found
}

/** The body of the block that opens at `open` (the index of its `{`). */
function block(text: string, open: number): string {
  let depth = 0
  for (let at = open; at < text.length; at++) {
    if (text[at] === "{") depth++
    else if (text[at] === "}" && --depth === 0) return text.slice(open + 1, at)
  }
  return text.slice(open + 1)
}

function unforwarded(name: string): string[] {
  const wrong: string[] = []
  const declared = declaring(name)
  for (const file of files) {
    const text = source(file)
    for (const component of declared) {
      // An instance: `Name {` or `id := Name {` or `...: Name {`, not the declaration and not an import.
      for (const match of text.matchAll(new RegExp(`(?<![\\w-])${component}\\s*\\{`, "g"))) {
        const before = text.slice(Math.max(0, match.index! - 40), match.index!)
        if (/component\s+$/.test(before) || /import\s*\{[^}]*$/.test(before) || /inherits\s+$/.test(before)) continue
        const body = block(text, match.index! + match[0].length - 1)
        // `inert: true`, written out: this instance is a picture and holds no target (a set's pooled tiles, the pick's card).
        if (/(?<![\w-])inert:\s*true\s*;/.test(body)) continue
        if (!new RegExp(`(?<![\\w-])${name}\\s*\\(`).test(body)) wrong.push(`${file}: an instance of ${component} does not forward ${name}`)
      }
    }
  }
  return wrong
}

describe("touch reaches the root", () => {
  for (const name of ["activated", "dragged"]) {
    test(`every instance of a component with \`${name}\` forwards it`, () => {
      expect(declaring(name).size).toBeGreaterThan(0)
      expect(unforwarded(name)).toEqual([])
    })
  }

  test("both roots declare the callbacks the shell binds", () => {
    const main = source("main.slint")
    const [surface, overlay] = main.split(/export component Overlay\b/) as [string, string]
    for (const root of [surface, overlay]) expect(root).toMatch(/callback activated\(target: string, at: float, released: bool\);/)
    expect(surface).toMatch(/callback dragged\(region: string, by: int\);/)
  })

  test("everything that draws a focus target can report a tap", () => {
    const wrong = files.filter(file => /accessible-item-selectable/.test(source(file)))
      .filter(file => !/VelvetTap\s*\{/.test(source(file)))
    expect(wrong).toEqual([])
  })
})

describe("the gesture rules", () => {
  const tap = source("ui/atoms/VelvetTap.slint")
  const finish = tap.slice(tap.indexOf("function finish"), tap.indexOf("pointer-event"))

  test("a cancel completes nothing", () => {
    expect(tap).toMatch(/event\.kind == PointerEventKind\.cancel\) \{\s*finish\(false\);/)
    // Every activation in `finish` stands under `armed && lifted`.
    const under = finish.slice(finish.indexOf("if (armed && lifted)"), finish.indexOf("if (armed && rowing)"))
    expect((finish.match(/root\.activated\(/g) ?? []).length).toBe((under.match(/root\.activated\(/g) ?? []).length)
  })

  test("a lift this area did not see begin completes nothing", () => {
    expect(tap).toMatch(/event\.kind == PointerEventKind\.down\) \{\s*armed = true;/)
    expect(finish).toMatch(/armed = false;/)
    expect(tap).toMatch(/moved => \{\s*if \(!armed\) \{\s*return;/)
  })

  test("a drag never activates: a moved press reports only a slide or a range", () => {
    expect(finish).toMatch(/\} else if \(!dragged\) \{/)
    expect(tap).toMatch(/Tokens\.drag-start/)
  })

  test("only the main button presses", () => {
    expect(tap).toMatch(/event\.button != PointerEventButton\.left && event\.kind != PointerEventKind\.cancel/)
  })
})

describe("the way back for a finger", () => {
  const main = source("main.slint")
  test("the stage holds the Back chip, shown only while touch is in use", () => {
    expect(main).toMatch(/VelvetBack\s*\{[^}]*shown: root\.way-back && root\.layer == "screen";/)
    expect(source("ui/atoms/VelvetBack.slint")).toMatch(/target: root\.shown \? "back" : "";/)
    expect(source("ui/atoms/VelvetBack.slint")).not.toMatch(/accessible-item-selectable/)
  })
  test("a tap beside a sheet or over the keyboard's field is Back", () => {
    expect(main).toMatch(/if root\.layer == "sheet" \|\| root\.layer == "keyboard": VelvetTap\s*\{[^}]*target: "back";/)
  })
})
