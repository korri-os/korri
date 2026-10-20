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
import { parseModule, visitNodes, type Node } from "./source-ast"

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

/** A value's space-separated parts, keeping a `calc(a * b)` whole. */
const topLevelTokens = (value: string): string[] => {
  const tokens: string[] = []
  let depth = 0
  let current = ""
  for (const char of value) {
    if (char === "(") depth += 1
    if (char === ")") depth -= 1
    if (/\s/.test(char) && depth === 0) {
      if (current !== "") tokens.push(current)
      current = ""
    } else {
      current += char
    }
  }
  if (current !== "") tokens.push(current)
  return tokens
}
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

describe("hard edges: flat colours, stairs and dither", () => {
  /**
   * Mania's menus are flat pixel shapes: a panel is black, its edge a solid
   * offset, a band colours meeting on a hard line, a veil a checker of
   * black. A blur, a blend, a smooth gradient or a soft shadow is the one
   * thing that would give Zone away as a web page. (The refinement brief
   * retired the old "eight colours" rule; the palettes are per screen
   * group now, and the rule that stays is: no blend.)
   */
  test("no stylesheet blurs, blends or grades a colour", () => {
    const offenders = cssFiles.flatMap((file) =>
      [...css(file).matchAll(/color-mix\(|radial-gradient\(|mix-blend-mode|backdrop-filter|blur\(|drop-shadow\(/g)]
        .map((match) => `${rel(file)}: ${match[0]}`))
      .sort()

    expect(offenders).toEqual([])
  })

  /**
   * A gradient is allowed only as a hard-stop pattern: every stop names two
   * positions, and each stop starts where the one before it ended, so no
   * two colours ever blend. A linear one is a stripe or a two-tone; a conic
   * one is a dither or a checker (the veil, PAUSED's stand-in). The first
   * part may be an angle, a side, `from` an angle, or an angle token
   * (`var(--zone-stripe-angle)`), so the lean stays a knob.
   */
  test("every linear-gradient and conic-gradient is hard-stop", () => {
    const offenders = cssFiles.flatMap((file) =>
      [...css(file).matchAll(/(linear|conic)-gradient\(([^()]*(?:\([^()]*(?:\([^()]*\)[^()]*)*\)[^()]*)*)\)/g)].flatMap((match) => {
        const parts = (match[2] ?? "").split(/,(?![^(]*\))/).map((part) => part.trim())
        const stops = parts.filter((part, index) =>
          !/^(?:-?[\d.]+(?:deg|turn)|to\s|from\s|at\s)/.test(part) && !(index === 0 && /^var\(--[a-z0-9-]*angle\)$/.test(part)))
        const edges = stops.map((stop) => stop.match(/\s(-?[\d.]+(?:%|deg|turn)?)\s+(-?[\d.]+(?:%|deg|turn)?)$/))
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
          const lengths = topLevelTokens(shadow.trim().replace(/^inset\s+/, ""))
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

describe("the ground is the one drawing", () => {
  /**
   * The canvas ground is drawing, not state: it paints from refs and the
   * computed style and re-renders nothing (REFINEMENT.md, 2b). It is the
   * only place allowed to draw on a canvas or run a frame clock; anything
   * else that animates does it in a stylesheet, where reduced motion stops
   * it, and anything else that shows a value shows it from the model.
   *
   * The one other clock is the moments' (use-zone-moment.ts): the brief's
   * structural and idle motion (REFINEMENT.md, 2d) is a seekable render(t),
   * which a stylesheet transition cannot be. It draws on no canvas, writes
   * only custom properties, and paints one rest frame under reduced motion.
   */
  const CANVAS_ALLOWANCE = ["ui/organisms/ZoneGround.tsx", "use-zone-ground.ts", "use-zone-moment.ts"]

  test("only ZoneGround, its hook and the moments' clock draw on a canvas or run a frame clock", () => {
    const offenders = tsxFiles
      .filter((file) => !file.endsWith(".part.tsx"))
      .filter((file) => /getContext\(|requestAnimationFrame\(|<canvas\b/.test(read(file)))
      .map(rel)
      .filter((file) => !CANVAS_ALLOWANCE.includes(file))
      .sort()
    expect(offenders).toEqual([])
  })

  test("the allowance names files that exist and still draw", () => {
    for (const file of CANVAS_ALLOWANCE) {
      expect(/getContext\(|requestAnimationFrame\(|<canvas\b/.test(read(join(SRC, file)))).toBe(true)
    }
  })
})

describe("one clock", () => {
  /**
   * The time is read in one place: the moments' seekable clock
   * (use-zone-moment.ts: `startZoneClock`, which every moment and the
   * ground's drift run on, and the M2 transition's mark). A second reading
   * of the time is a clock `window.seek` cannot hold (the ground's drift
   * was one), and a value drawn from the time is a fact Korri did not
   * publish (Zone shows Korri's `clockLabel`, never its own). So no other
   * file under src/ (parts and fixtures included) reads `Date.now()`,
   * `performance.now()`, `new Date()` with no argument or `Date()`.
   */
  const CLOCK_SEAMS = ["use-zone-moment.ts"]

  const readsTheTime = (node: Node): boolean => {
    const named = (value: unknown, name: string) =>
      typeof value === "object" && value !== null && (value as { type?: string; name?: string }).type === "Identifier"
      && (value as { name?: string }).name === name
    if (node.type === "MemberExpression") {
      const { object, property } = node as unknown as { object: Node; property: Node }
      if (!named(property, "now")) return false
      if (named(object, "Date") || named(object, "performance")) return true
      return object.type === "MemberExpression" && named((object as unknown as { property: Node }).property, "performance")
    }
    if (node.type === "NewExpression") {
      const { callee, arguments: args } = node as unknown as { callee: Node; arguments: readonly Node[] }
      return named(callee, "Date") && args.length === 0
    }
    if (node.type === "CallExpression") return named((node as unknown as { callee: Node }).callee, "Date")
    return false
  }

  const timeReaders = (files: readonly string[]) =>
    files
      .filter((file) => {
        let found = false
        visitNodes(parseModule(file, read(file)), (node) => {
          if (readsTheTime(node)) found = true
        })
        return found
      })
      .map(rel)
      .sort()

  test("no file outside the clock seam reads the time", () => {
    expect(timeReaders(tsxFiles).filter((file) => !CLOCK_SEAMS.includes(file))).toEqual([])
  })

  test("the seam is named because it reads the time", () => {
    expect(timeReaders(CLOCK_SEAMS.map((file) => join(SRC, file)))).toEqual(CLOCK_SEAMS)
  })

  test("the rule sees every way to read the time, and lets a given date pass", () => {
    const probe = (text: string) => {
      let found = false
      visitNodes(parseModule("probe.ts", text), (node) => {
        if (readsTheTime(node)) found = true
      })
      return found
    }
    expect(["Date.now()", "performance.now()", "window.performance.now()", "new Date()", "Date()"].map(probe))
      .toEqual([true, true, true, true, true])
    expect(["new Date(at)", "date.getDate()", "now()"].map(probe)).toEqual([false, false, false])
  })
})

describe("no word breaks inside itself", () => {
  /**
   * Zone's own words wrap between words or truncate with "…"; they never
   * break inside a word (REFINEMENT.md, 4: "Config/ured"). The one allowed
   * break is in a sentence Korri publishes, at the line's end, with a
   * hyphen where the language has one: `overflow-wrap: break-word` with
   * `hyphens: auto`, in the same rule.
   */
  test("no stylesheet breaks a word anywhere", () => {
    const offenders = cssFiles.flatMap((file) =>
      [...css(file).matchAll(/overflow-wrap:\s*anywhere|word-break:\s*break-(?:all|word)|line-break:\s*anywhere/g)]
        .map((match) => `${rel(file)}: ${match[0]}`))
    expect(offenders).toEqual([])
  })

  test("a word breaks at a line's end only in a rule that hyphenates: Korri's sentences", () => {
    const offenders = cssFiles.flatMap((file) =>
      [...css(file).matchAll(/([^{}]+)\{([^{}]*)\}/g)]
        .filter(([, , body]) => /overflow-wrap:\s*break-word/.test(body ?? "") && !/hyphens:\s*auto/.test(body ?? ""))
        .map(([, selector]) => `${rel(file)}: ${(selector ?? "").trim()}`))
    expect(offenders).toEqual([])
  })
})
