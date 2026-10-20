/**
 * Zone's authoring gate.
 *
 * Where the decomposition gate asks "is every unit a component with a part",
 * this one asks "is each part and component authored so it can be reviewed":
 * a part that yields no knobs looks fine in a gallery while offering nothing,
 * an inline style takes a decision out of the stylesheet, and a soft shadow or
 * a smooth gradient is the fastest way to make Mania's hard vector shapes read
 * as a web page again.
 *
 * Raw colours and pixel values are the atomic gate's business
 * (test/atomic-gate.test.ts).
 */
import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync, statSync } from "node:fs"
import { basename, join, relative } from "node:path"

const SRC = join(import.meta.dir, "..", "src")

/**
 * What Zone may import. The surface treaty is the whole contract with Korri.
 * `qrcode` is a pure encoder the identity backup draws with: it is a package
 * dependency of Zone itself, performs no effects, and would ship with Zone
 * from any repository. Anything else (Effect, atoms, a router, another
 * surface, a `@platform` module) would tie Zone to this repository and break
 * the promise that a surface can ship on its own.
 */
const ALLOWED_IMPORTS = ["react", "react-dom", "@contracts/surface", "qrcode"]

/** A package by its exact name, or a path inside it: `react` and `react/jsx-runtime`, never `react-foo`. */
const allowedImport = (specifier: string): boolean =>
  ALLOWED_IMPORTS.some((allowed) => specifier === allowed || specifier.startsWith(`${allowed}/`))

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
const css = (file: string) => read(file).replace(/\/\*[\s\S]*?\*\//g, "")

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
   * A sealed part earns knobs only when its root is one directly imported
   * component. A part that wraps children silently degrades to its authored
   * render: it still looks right and offers no controls at all, which is the
   * trap this assertion exists to catch. Page and template parts are
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
        const wrapsChildren = new RegExp(`</${root}>`).test(source)
        return !imported || wrapsChildren
      })
      .map(rel)
      .sort()

    expect(offenders).toEqual([])
  })
})

describe("visual decisions live in the stylesheet", () => {
  /**
   * An inline style beats every class the theme can state, so a component
   * that sets one has taken a decision out of the stylesheet where nobody can
   * override or review it.
   */
  test("no component sets an inline style", () => {
    const offenders = tsxFiles
      .filter((file) => /style=\{\{|CSSProperties/.test(read(file)))
      .map(rel)
      .sort()

    expect(offenders).toEqual([])
  })
})

/**
 * Zone never translates a device (AGENTS.md: the portal is hardware-blind
 * and does it for every surface). A surface that reads a key, or calls
 * preventDefault on one, makes the portal's window listener drop it and
 * skip its repeat and release handling. Fixtures stand in for the portal
 * in Caliper and are exempt.
 */
describe("the surface reads no key", () => {
  const outsideFixtures = tsxFiles.filter((file) => !rel(file).startsWith("fixtures/"))
  const code = (file: string) => read(file).replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "")

  test("no file outside src/fixtures reads event.key, .key ===, keyCode or an arrow key's name", () => {
    const offenders = outsideFixtures
      .filter((file) => /\bevent\.key\b|\.key\s*===|\bkeyCode\b|\bArrow(Left|Right|Up|Down)\b/.test(code(file)))
      .map(rel)
    expect(offenders).toEqual([])
  })

  test("no key handler outside src/fixtures calls preventDefault", () => {
    const offenders = outsideFixtures
      .filter((file) => /keydown|keyup|onKeyDown|onKeyUp|KeyboardEvent/.test(code(file)) && /preventDefault/.test(code(file)))
      .map(rel)
    expect(offenders).toEqual([])
  })
})

describe("the surface treaty is the whole contract", () => {
  const IMPORT = /from\s+"([^"]+)"/g

  test("nothing is imported outside the treaty, React, and qrcode", () => {
    const offenders = tsxFiles
      .flatMap((file) => {
        const source = read(file)
        return [...source.matchAll(IMPORT)]
          .map((match) => match[1])
          .filter((specifier): specifier is string => specifier !== undefined)
          .filter((specifier) => !specifier.startsWith(".") && !allowedImport(specifier))
          .map((specifier) => `${rel(file)}: ${specifier}`)
      })
      .sort()

    expect(offenders).toEqual([])
  })

  test("the allowlist matches a package's exact name or a path inside it, never a name it starts", () => {
    for (const specifier of ["react", "react/jsx-runtime", "react-dom", "react-dom/client", "@contracts/surface/korri-surface", "qrcode"]) {
      expect(allowedImport(specifier)).toBe(true)
    }
    for (const specifier of ["react-foo", "reactive", "react-dom-extra", "@contracts/surfaces", "@contracts/surface-x/y", "qrcode-svg", "@contracts/generated/korrid"]) {
      expect(allowedImport(specifier)).toBe(false)
    }
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

describe("design inputs are addressable", () => {
  /**
   * Caliper lists every custom property a stylesheet declares, keyed by
   * property and selector. Declaring one twice for the same selector gives
   * two controls one key, and the design tool's own list breaks.
   */
  test("no selector declares the same custom property twice", () => {
    const offenders = cssFiles.flatMap((file) => {
      const source = css(file)
      const seen = new Map<string, number>()
      for (const [, selector, body] of source.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        for (const [, property] of (body ?? "").matchAll(/(--[a-z0-9-]+)\s*:/g)) {
          const key = `${(selector ?? "").trim()} ${property}`
          seen.set(key, (seen.get(key) ?? 0) + 1)
        }
      }
      return [...seen.entries()].filter(([, count]) => count > 1).map(([key]) => `${rel(file)}: ${key}`)
    })

    expect(offenders.sort()).toEqual([])
  })

  /**
   * A knob is a literal custom property with a `@label` comment above it. A
   * token written as `calc()` or `clamp()` is an output, and Caliper shows no
   * control for it, so every token in zone-tokens.css must be literal and
   * every one must carry its knob hint. A role written as `var(--zone-blue)`
   * is literal: Caliper offers it as a picker of the sibling tokens.
   */
  test("every token in zone-tokens.css is a literal with a knob hint", () => {
    const source = read(join(SRC, "zone-tokens.css"))
    const offenders: string[] = []
    const lines = source.split("\n")
    lines.forEach((line, index) => {
      const declaration = line.match(/^\s*(--[a-z0-9-]+)\s*:\s*([^;]+);/)
      if (declaration === null) return
      const [, name, value] = declaration
      if (/\b(?:calc|clamp|min|max)\(/.test(value ?? "")) offenders.push(`${name}: computed`)
      const hint = lines[index - 1] ?? ""
      if (!/\/\*\*.*@label\b/.test(hint)) offenders.push(`${name}: no @label hint`)
    })
    expect(offenders).toEqual([])
  })
})

describe("eight colours, hard edges", () => {
  /**
   * Mania's menus are flat vector shapes: a panel is black, its edge is a
   * solid offset in blue, a band is three colours meeting on a hard line.
   * A blur, a blend, a smooth gradient or a soft shadow is the one thing that
   * would give Zone away as a web page.
   */
  test("no stylesheet blurs, blends or grades a colour", () => {
    const offenders = cssFiles.flatMap((file) =>
      [...css(file).matchAll(/color-mix\(|radial-gradient\(|conic-gradient\(|mix-blend-mode|backdrop-filter|blur\(|drop-shadow\(/g)]
        .map((match) => `${rel(file)}: ${match[0]}`))
      .sort()

    expect(offenders).toEqual([])
  })

  /**
   * A linear-gradient is allowed only as a stripe pattern: every stop names
   * two positions, and each stop starts where the one before it ended, so no
   * two colours ever blend. The first part may be an angle, a side, or an
   * angle token (`var(--zone-stripe-angle)`), so the lean stays a knob.
   */
  test("every linear-gradient is a hard-stop stripe", () => {
    const offenders = cssFiles.flatMap((file) =>
      [...css(file).matchAll(/linear-gradient\(([^()]*(?:\([^()]*\)[^()]*)*)\)/g)].flatMap((match) => {
        const parts = (match[1] ?? "").split(/,(?![^(]*\))/).map((part) => part.trim())
        const stops = parts.filter((part, index) =>
          !/^(?:-?[\d.]+(?:deg|turn)|to\s)/.test(part) && !(index === 0 && /^var\(--[a-z0-9-]*angle\)$/.test(part)))
        const edges = stops.map((stop) => stop.match(/\s(-?[\d.]+%?)\s+(-?[\d.]+%?)$/))
        if (edges.some((edge) => edge === null)) return [`${rel(file)}: ${match[0]} (a stop with one position)`]
        for (let index = 1; index < edges.length; index += 1) {
          if (edges[index]?.[1] !== edges[index - 1]?.[2]) {
            return [`${rel(file)}: ${match[0]} (stops do not meet)`]
          }
        }
        return []
      }))
      .sort()

    expect(offenders).toEqual([])
  })

  /** A box-shadow may offset. It may not blur: its third length, if written, is 0. */
  test("no box-shadow has a blur radius", () => {
    const offenders = cssFiles.flatMap((file) =>
      [...css(file).matchAll(/box-shadow\s*:\s*([^;]+);/g)].flatMap((match) =>
        (match[1] ?? "").split(/,(?![^(]*\))/).flatMap((shadow) => {
          const lengths = shadow.trim().replace(/^inset\s+/, "").split(/\s+/)
            .filter((token) => /^-?[\d.]+(?:px|em|rem|%)?$/.test(token) || /^(?:calc|var)\(/.test(token))
          const blur = lengths[2]
          return blur !== undefined && blur !== "0" ? [`${rel(file)}: ${shadow.trim()}`] : []
        })))
      .sort()

    expect(offenders).toEqual([])
  })
})

describe("fixtures stay out of the product", () => {
  /**
   * src/fixtures holds stand-in data and a fixture d-pad that reads arrow keys
   * for Caliper. The key-reading gate exempts that folder, so the exemption
   * is only safe while nothing the product ships imports from it. Parts and
   * tests may; nothing else may.
   */
  test("no product file imports from src/fixtures", () => {
    const product = allFiles.filter((file) =>
      /\.(?:ts|tsx)$/.test(file)
      && !file.endsWith(".part.tsx")
      && !rel(file).startsWith("fixtures/"))
    const offenders = product.filter((file) => /from\s+"[^"]*\/fixtures\//.test(read(file))).map(rel)
    expect(offenders).toEqual([])
  })
})
