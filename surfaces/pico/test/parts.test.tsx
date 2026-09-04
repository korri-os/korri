/**
 * Every part renders something.
 *
 * A part is discovered by filename, so a broken one is not a compile error — it
 * is a blank tile in a gallery that nobody notices for a week. This walks the
 * same filesystem the catalog does and asserts each part renders real content,
 * because "it mounted" is exactly what a part with unreachable data also does.
 */
import { afterEach, expect, test } from "bun:test"
import { cleanup, render } from "@testing-library/react"
import { readdirSync, statSync } from "node:fs"
import { join, relative } from "node:path"
import type { ComponentType } from "react"

const SRC = join(import.meta.dir, "..", "src")

function walk(dir: string): readonly string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry)
    return statSync(full).isDirectory() ? walk(full) : [full]
  })
}

const parts = walk(SRC)
  .filter((file) => file.endsWith(".part.tsx"))
  .sort()

/**
 * Parts whose whole content is a shape. They are verified by eye, not by text —
 * listing them here keeps that an explicit decision rather than a blanket
 * weakening of the assertion for every other part.
 */
const TEXTLESS = new Set(["ui/atoms/PicoPixelDisc.atom.part.tsx"])

afterEach(() => cleanup())

test("the surface publishes parts at all", () => {
  // Guards the walk itself: an empty list would make every case below vacuous.
  expect(parts.length).toBeGreaterThan(0)
})

test.each(parts.map((file) => [relative(SRC, file), file]))(
  "%s renders content",
  async (_label, file) => {
    const part = (await import(file)) as {
      default: ComponentType
      name?: string
    }

    const { container } = render(<part.default />)

    if (TEXTLESS.has(_label)) {
      expect(container.querySelectorAll("svg > *").length).toBeGreaterThan(0)
    } else {
      expect(container.textContent?.trim().length ?? 0).toBeGreaterThan(0)
    }
    expect(part.name).toBeTruthy()
  },
)
