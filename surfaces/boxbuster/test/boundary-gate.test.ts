/**
 * Boxbuster's boundary gate.
 *
 * A surface may import the treaty and nothing else from Korri, so it can ship
 * from another repository unchanged. Inside Boxbuster, the treaty stops at the
 * root: one component reads the model, one module converts it, and the mount
 * adapter types the entry point. Everything else speaks in tapes and rooms.
 */
import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"

const SRC = join(import.meta.dir, "..", "src")

/** Packages a source file may import. `@react-three/drei` is deliberately
 * absent: legacy used it only for pointer lock, which a kiosk never uses. */
const ALLOWED_PACKAGES = [
  "react",
  "react-dom/client",
  "three",
  "@react-three/fiber",
]

const TREATY = "@contracts/surface/korri-surface"

/** The only files allowed to name a treaty type. */
const TREATY_READERS = [
  "BoxbusterSurface.tsx",
  "boxbuster-store-view.ts",
  "mount.tsx",
]

const sources = readdirSync(SRC).filter(name => /\.(ts|tsx)$/.test(name))

function importsOf(file: string): string[] {
  const text = readFileSync(join(SRC, file), "utf8")
  return [...text.matchAll(/from\s+"([^"]+)"|import\s+"([^"]+)"/g)].map(
    match => match[1] ?? match[2] ?? "",
  )
}

describe("the boundary gate", () => {
  test("finds the sources it guards", () => {
    expect(sources).toContain("BoxbusterSurface.tsx")
    expect(sources).toContain("scene.tsx")
  })

  test("imports nothing outside the treaty, React, and three.js", () => {
    const strays = sources.flatMap(file =>
      importsOf(file)
        .filter(
          spec =>
            !spec.startsWith("./") &&
            spec !== TREATY &&
            !ALLOWED_PACKAGES.includes(spec),
        )
        .map(spec => `${file}: ${spec}`),
    )
    expect(strays).toEqual([])
  })

  test("the treaty stops at the surface root", () => {
    const readers = sources.filter(file => importsOf(file).includes(TREATY))
    expect(readers.sort()).toEqual([...TREATY_READERS].sort())
  })
})
