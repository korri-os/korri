/**
 * Pico's decomposition gate.
 *
 * This file exists to fail. It is written before the components it governs, and
 * its failure list is the work list: every unit Pico renders must be a real
 * component with a real part beside it, at every layer, with no second
 * component hiding in a file and no visual decision duplicated across files.
 *
 * Prose cannot enforce decomposition — careful review of a single screen still
 * misses units. A gate finds them on every run, so this is the authority and
 * the README defers to it.
 */
import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync, statSync } from "node:fs"
import { basename, dirname, join, relative } from "node:path"

const SRC = join(import.meta.dir, "..", "src")

const LAYERS = ["page", "template", "organism", "molecule", "atom"] as const
type Layer = (typeof LAYERS)[number]

/**
 * Files that render no surface of their own and therefore own no part.
 *
 * `PicoSurface.tsx` and `mount.tsx` are bindings: they read the treaty model and
 * hand plain values down. `fixtures/` is fixture data. `fonts/` is binary. Each
 * exclusion is a decision, not a convenience — a component added here would be
 * invisible to the gate, so nothing that renders may be listed.
 */
const NON_RENDERING = [
  "PicoSurface.tsx",
  "mount.tsx",
  "index.ts",
  "fixtures",
  "fonts",
]

function walk(dir: string): readonly string[] {
  const entries = readdirSync(dir)
  return entries.flatMap((entry) => {
    const full = join(dir, entry)
    return statSync(full).isDirectory() ? walk(full) : [full]
  })
}

const allFiles = walk(SRC)

function isExcluded(file: string): boolean {
  const rel = relative(SRC, file)
  return NON_RENDERING.some(
    (skip) => rel === skip || rel.startsWith(`${skip}/`),
  )
}

/** Every `.tsx` that renders something and is not itself a part. */
const componentFiles = allFiles.filter(
  (file) =>
    file.endsWith(".tsx") &&
    !file.endsWith(".part.tsx") &&
    !file.endsWith(".test.tsx") &&
    !isExcluded(file),
)

const partFiles = allFiles.filter((file) => file.endsWith(".part.tsx"))

function partLayer(file: string): string | undefined {
  return basename(file).split(".").at(-3)
}

describe("every rendered unit is a component with a part beside it", () => {
  test("no component file lacks a sibling part", () => {
    const withoutPart = componentFiles
      .filter((file) => {
        const name = basename(file, ".tsx")
        return !partFiles.some(
          (part) =>
            dirname(part) === dirname(file) &&
            basename(part).startsWith(`${name}.`),
        )
      })
      .map((file) => relative(SRC, file))
      .sort()

    expect(withoutPart).toEqual([])
  })

  test("no part is orphaned from a component", () => {
    const orphans = partFiles
      .filter((part) => {
        const name = basename(part).split(".")[0]
        return !componentFiles.some(
          (file) =>
            dirname(file) === dirname(part) &&
            basename(file, ".tsx") === name,
        )
      })
      .map((part) => relative(SRC, part))
      .sort()

    expect(orphans).toEqual([])
  })

  test("every part declares one of the five layers", () => {
    const badLayers = partFiles
      .filter((part) => !LAYERS.includes(partLayer(part) as Layer))
      .map((part) => relative(SRC, part))
      .sort()

    expect(badLayers).toEqual([])
  })
})

describe("the surface is decomposed at every layer, not just the convenient ones", () => {
  const layers = new Set(partFiles.map(partLayer))

  test.each(["page", "template", "molecule", "atom"])(
    "a %s part exists",
    (layer) => {
      expect([...layers].sort()).toContain(layer)
    },
  )
})

describe("one file holds one component", () => {
  /**
   * A second render function in a file is a component nobody named, so it can
   * never be reviewed, reused, or previewed. PascalCase only: SCREAMING_CASE is
   * a constant and lowercase is a helper.
   */
  const COMPONENT_DECL =
    /^(?:export )?(?:function|const) ([A-Z][a-z0-9][A-Za-z0-9]*)\b/gm

  test("no file declares a second component", () => {
    const offenders = componentFiles
      .map((file) => {
        const names = [
          ...readFileSync(file, "utf8").matchAll(COMPONENT_DECL),
        ].map((match) => match[1])
        return { file: relative(SRC, file), names }
      })
      .filter((entry) => entry.names.length > 1)
      .map((entry) => `${entry.file}: ${entry.names.join(", ")}`)
      .sort()

    expect(offenders).toEqual([])
  })
})

describe("no visual decision is duplicated across files", () => {
  const CLASS_LITERAL = /className="([^"]+)"/g

  test("no className literal appears in two component files", () => {
    const owners = new Map<string, string[]>()
    for (const file of componentFiles) {
      const source = readFileSync(file, "utf8")
      for (const [, value] of source.matchAll(CLASS_LITERAL)) {
        if (value === undefined) continue
        const seen = owners.get(value) ?? []
        seen.push(relative(SRC, file))
        owners.set(value, seen)
      }
    }

    const duplicated = [...owners.entries()]
      .filter(([, files]) => new Set(files).size > 1)
      .map(([value, files]) => `"${value}" in ${[...new Set(files)].join(", ")}`)
      .sort()

    expect(duplicated).toEqual([])
  })

  const cssFiles = allFiles.filter((file) => file.endsWith(".css"))
  const CLASS_SELECTOR = /\.([a-z][a-z0-9-]*)/g

  test("no class selector is defined in two stylesheets", () => {
    const owners = new Map<string, Set<string>>()
    for (const file of cssFiles) {
      /* Comments name classes while explaining them; only real selectors count. */
      const source = readFileSync(file, "utf8").replace(/\/\*[\s\S]*?\*\//g, "")
      for (const [, name] of source.matchAll(CLASS_SELECTOR)) {
        if (name === undefined) continue
        const seen = owners.get(name) ?? new Set<string>()
        seen.add(relative(SRC, file))
        owners.set(name, seen)
      }
    }

    const duplicated = [...owners.entries()]
      .filter(([, files]) => files.size > 1)
      .map(([name, files]) => `.${name} in ${[...files].sort().join(", ")}`)
      .sort()

    expect(duplicated).toEqual([])
  })
})

describe("every component has a consumer", () => {
  /**
   * Legacy's kit was 94 components imported by zero routed screens. A gallery
   * makes an orphan look finished, so the rule is structural: every atom,
   * molecule, organism and template must be imported by some other component
   * or page — its part alone does not count, because a part exists to review
   * the component, not to use it. Pages are consumed by the surface binding and
   * are checked separately.
   */
  const importedNames = new Set<string>()
  for (const file of [...componentFiles, join(SRC, "PicoSurface.tsx")]) {
    const src = readFileSync(file, "utf8")
    for (const m of src.matchAll(/from\s+"\.[^"]*\/([A-Za-z0-9]+)"/g)) {
      importedNames.add(m[1])
    }
  }

  test("no atom, molecule, organism or template is imported by nothing", () => {
    const orphans = componentFiles
      .filter((file) => /\/ui\/(atoms|molecules|organisms|templates)\//.test(file))
      .map((file) => basename(file, ".tsx"))
      .filter((name) => !importedNames.has(name))
    expect(orphans).toEqual([])
  })

  test("every page is reached from the surface binding", () => {
    const binding = readFileSync(join(SRC, "PicoSurface.tsx"), "utf8")
    const pages = componentFiles
      .filter((file) => file.includes("/pages/"))
      .map((file) => basename(file, ".tsx"))
    const reachable = new Set<string>()
    const queue = [binding]
    while (queue.length > 0) {
      const src = queue.pop()!
      for (const m of src.matchAll(/from\s+"(\.[^"]*)"/g)) {
        const name = m[1].split("/").at(-1)!
        if (reachable.has(name)) continue
        reachable.add(name)
        const hit = componentFiles.find((f) => basename(f, ".tsx") === name)
        if (hit) queue.push(readFileSync(hit, "utf8"))
      }
    }
    expect(pages.filter((p) => !reachable.has(p))).toEqual([])
  })
})

describe("parts demonstrate only what Korri publishes", () => {
  /**
   * A part that hand-writes `{ id, title, friends: 4 }` proves a field Korri
   * does not have, and a reviewer approving it approves a screen that can only
   * ever show fixtures. Parts therefore draw their game and model data from
   * `fixtures/`, which is typed against `SurfaceModel` — so inventing a field is
   * a type error there rather than a design decision here.
   */
  test("no part builds a game or model literal of its own", () => {
    const offenders = partFiles.filter((file) => {
      const src = readFileSync(file, "utf8")
      const buildsGame = /\b(id|title):\s*"[^"]+",?\s*\n?\s*(title|id|coverArtUrl|section):/.test(src)
      const importsFixtures = /from\s+"[^"]*fixtures\//.test(src)
      return buildsGame && !importsFixtures
    })
    expect(offenders.map((f) => relative(SRC, f))).toEqual([])
  })
})
