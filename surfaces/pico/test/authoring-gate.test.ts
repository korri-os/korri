/**
 * Pico's authoring gate.
 *
 * Where the decomposition gate asks "is every unit a component with a part",
 * this one asks "is each part and component authored so it can actually be
 * reviewed": a sealed part that yields no Inspector controls looks fine in a
 * gallery while offering nothing, and a raw hex buried in a component is
 * duplication at a scale review never catches.
 */
import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync, statSync } from "node:fs"
import { basename, join, relative } from "node:path"

const SRC = join(import.meta.dir, "..", "src")

/**
 * The only stylesheet allowed to state a raw colour or pixel value. Every other
 * file consumes the scale through `var()`, so a brand change is one edit.
 */
const TOKENS_CSS = "pico-tokens.css"

/**
 * What Pico may import. The surface treaty is the whole contract with Korri;
 * the intrinsic core is shared design maths. Anything else — Effect, atoms, a
 * router, another surface, a `@platform` module — would tie Pico to this
 * repository and break the promise that a surface can ship on its own.
 */
const ALLOWED_IMPORTS = [
  "react",
  "react-dom",
  "react/jsx-runtime",
  "react-dom/client",
  "@contracts/surface/",
  "@korri/intrinsic-design",
]

function walk(dir: string): readonly string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry)
    return statSync(full).isDirectory() ? walk(full) : [full]
  })
}

const allFiles = walk(SRC)
const tsxFiles = allFiles.filter(
  (file) => file.endsWith(".tsx") || file.endsWith(".ts"),
)
const partFiles = allFiles.filter((file) => file.endsWith(".part.tsx"))
const cssFiles = allFiles.filter((file) => file.endsWith(".css"))

const read = (file: string) => readFileSync(file, "utf8")
const rel = (file: string) => relative(SRC, file)

describe("every part is authored so the catalog can use it", () => {
  test("each part default-exports a function", () => {
    const offenders = partFiles
      .filter((file) => !/export default function [A-Z]/.test(read(file)))
      .map(rel)
      .sort()

    expect(offenders).toEqual([])
  })

  test("each part exports a name", () => {
    const offenders = partFiles
      .filter((file) => !/export const name = "/.test(read(file)))
      .map(rel)
      .sort()

    expect(offenders).toEqual([])
  })

  /**
   * A sealed part earns Inspector controls only when its root is one directly
   * imported component. A part that wraps children silently degrades to its
   * authored render — it still looks right and offers no controls at all, which
   * is the trap this assertion exists to catch. Page and template parts are
   * compositions by definition and are exempt.
   */
  test("each atom, molecule, and organism part roots in one imported component", () => {
    const sealed = partFiles.filter((file) => {
      const layer = basename(file).split(".").at(-3)
      return layer === "atom" || layer === "molecule" || layer === "organism"
    })

    const offenders = sealed
      .filter((file) => {
        const source = read(file)
        const root = source.match(/return\s*\(?\s*<([A-Za-z][\w.]*)/)?.[1]
        if (root === undefined) return true
        if (!/^[A-Z]/.test(root)) return true
        const imported = new RegExp(
          `import\\s*\\{[^}]*\\b${root}\\b[^}]*\\}\\s*from`,
        ).test(source)
        /* A closing tag means the root wraps children, so generated placement
         * could only re-render the wrapper and would drop what is inside —
         * which is exactly the part that silently offers no controls. */
        const wrapsChildren = new RegExp(`</${root}>`).test(source)
        return !imported || wrapsChildren
      })
      .map(rel)
      .sort()

    expect(offenders).toEqual([])
  })
})

describe("visual decisions live in tokens", () => {
  test("no component states a raw colour or pixel value", () => {
    const offenders = tsxFiles
      .flatMap((file) => {
        const found = [...read(file).matchAll(/#[0-9a-fA-F]{3,8}\b|\b\d+px\b/g)]
        return found.map((match) => `${rel(file)}: ${match[0]}`)
      })
      .sort()

    expect(offenders).toEqual([])
  })

  test(`no stylesheet outside ${TOKENS_CSS} states a raw colour or pixel value`, () => {
    const offenders = cssFiles
      .filter((file) => basename(file) !== TOKENS_CSS)
      .flatMap((file) => {
        const found = [...read(file).matchAll(/#[0-9a-fA-F]{3,8}\b|\b\d+px\b/g)]
        return found.map((match) => `${rel(file)}: ${match[0]}`)
      })
      .sort()

    expect(offenders).toEqual([])
  })

  /**
   * An inline style beats every class the theme can state, so a component that
   * sets one has taken a decision out of the stylesheet where nobody can
   * override or review it. Legacy Pico's own notes record this as the bug that
   * broke its selection highlight.
   */
  test("no component sets an inline style", () => {
    const offenders = tsxFiles
      .filter((file) => /style=\{\{|CSSProperties/.test(read(file)))
      .map(rel)
      .sort()

    expect(offenders).toEqual([])
  })
})

describe("the surface treaty is the whole contract", () => {
  const IMPORT = /from\s+"([^"]+)"/g

  test("nothing is imported outside the treaty, React, and the intrinsic core", () => {
    const offenders = tsxFiles
      .flatMap((file) => {
        const source = read(file)
        return [...source.matchAll(IMPORT)]
          .map((match) => match[1])
          .filter((specifier): specifier is string => specifier !== undefined)
          .filter(
            (specifier) =>
              !specifier.startsWith(".") &&
              !ALLOWED_IMPORTS.some(
                (allowed) =>
                  specifier === allowed || specifier.startsWith(allowed),
              ),
          )
          .map((specifier) => `${rel(file)}: ${specifier}`)
      })
      .sort()

    expect(offenders).toEqual([])
  })

  test("no design-part registry is reintroduced", () => {
    const offenders = allFiles
      .filter((file) => /design-parts?\./.test(basename(file)))
      .map(rel)
      .sort()

    expect(offenders).toEqual([])
  })

  test("no story files shadow the parts catalog", () => {
    const offenders = allFiles
      .filter((file) => file.endsWith(".story.tsx"))
      .map(rel)
      .sort()

    expect(offenders).toEqual([])
  })
})
