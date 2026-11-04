/**
 * MENU's list has no clipping ancestor between it and the window
 * (intrinsic design, rule 8): main.slint mounts it at the window's top
 * level, outside the clipped screen rectangles.
 */
import { expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { ROOT, stringsBlanked } from "./slint-source"

/** The start of every block that encloses `at`, innermost first. */
function enclosing(text: string, at: number): number[] {
  const starts: number[] = []
  let depth = 0
  for (let i = at - 1; i >= 0; i -= 1) {
    if (text[i] === "}") depth += 1
    else if (text[i] === "{") {
      if (depth === 0) starts.push(i)
      else depth -= 1
    }
  }
  return starts
}

/** A block's own text, its nested blocks left out. */
function own(text: string, start: number): string {
  let depth = 0
  let out = ""
  for (let i = start + 1; i < text.length; i += 1) {
    if (text[i] === "{") depth += 1
    else if (text[i] === "}") {
      if (depth === 0) break
      depth -= 1
    } else if (depth === 0) out += text[i]
  }
  return out
}

test("MENU's list is mounted with no clipping ancestor", () => {
  const text = stringsBlanked(readFileSync(join(ROOT, "main.slint"), "utf8"))
  const surface = text.indexOf("export component Surface")
  const at = text.indexOf("PicoMenu {", surface)
  expect(at).toBeGreaterThan(surface)
  const blocks = enclosing(text, at)
  // Directly in the window: the only enclosing block is Surface itself.
  expect(blocks.length).toBe(1)
  expect(text.slice(text.lastIndexOf("export component", blocks[0]), blocks[0])).toContain("Surface inherits Window")
  for (const start of blocks) expect(own(text, start)).not.toMatch(/\bclip\s*:\s*true/)
})

test("the check sees a clipping ancestor when there is one", () => {
  const text = "export component Surface inherits Window { Rectangle { clip: true; if open : PicoMenu { x: 0; } } }"
  const blocks = enclosing(text, text.indexOf("PicoMenu {"))
  expect(blocks.length).toBe(2)
  expect(blocks.some(start => /\bclip\s*:\s*true/.test(own(text, start)))).toBe(true)
})
