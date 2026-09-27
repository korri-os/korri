import { afterEach, expect, test } from "bun:test"
import { cleanup, render } from "@testing-library/react"
import { readdirSync } from "node:fs"
import { join, relative } from "node:path"
import type { ComponentType } from "react"

const src = join(import.meta.dir, "..", "src")
function partsIn(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name)
    return entry.isDirectory() ? partsIn(path) : path.endsWith(".part.tsx") ? [path] : []
  })
}

// These two ornaments have no inputs. A second export would repeat the same state.
const singleStateParts = new Set([
  "ui/atoms/PicoPaletteBar.atom.part.tsx",
  "ui/atoms/PicoPixelDisc.atom.part.tsx",
])

// These inputs intentionally unmount the real component. Keep the exceptions
// specific so an accidental blank state elsewhere fails this gate.
const emptyStates = new Set([
  "ui/organisms/PicoCartShelf.organism.part.tsx/EmptyLibrary",
  "ui/organisms/PicoGameActions.organism.part.tsx/NoActions",
  "ui/organisms/PicoIdentityDialog.organism.part.tsx/Closed",
  "ui/organisms/PicoResumeList.organism.part.tsx/NoResumableGames",
])

const discoveredStates = new Set<string>()
const discoveredParts = new Set<string>()
afterEach(cleanup)

for (const file of partsIn(src).sort()) {
  const path = relative(src, file)
  discoveredParts.add(path)
  const exports = await import(file)
  const states = Object.entries(exports).filter(
    ([name, value]) => /^[A-Z]/.test(name) && typeof value === "function",
  ) as [string, ComponentType][]

  test(`${path} covers alternate inputs or is a fixed ornament`, () => {
    expect(states.length > 0 || singleStateParts.has(path)).toBe(true)
  })

  for (const [name, State] of states) {
    discoveredStates.add(`${path}/${name}`)
    test(`${path} / ${name} mounts without required props`, () => {
      const { container } = render(<State />)
      expect(container.childElementCount > 0).toBe(!emptyStates.has(`${path}/${name}`))
    })
  }
}

test("every documented exception still belongs to a discovered part or state", () => {
  expect(discoveredParts.size).toBeGreaterThan(0)
  expect([...singleStateParts].filter(path => !discoveredParts.has(path))).toEqual([])
  expect([...emptyStates].filter(path => !discoveredStates.has(path))).toEqual([])
})
