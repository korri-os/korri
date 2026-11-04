/**
 * Pico's decomposition, held in .slint as it was in React
 * (surfaces/pico/test/decomposition-gate.test.ts):
 *
 * - every React component is one Slint component at the same layer, in its
 *   own file, unless MERGED says where it went and why;
 * - every .slint file under ui/ and pages/ declares exactly one component,
 *   named as the file, and nothing that is not in the tree or ADDED;
 * - every component has a preview file beside it that imports it and draws
 *   it, as a Caliper part did;
 * - imports point down or stay within a layer, never up, and never into
 *   main.slint;
 * - shapes read no state: only ui/tokens.slint and ui/palette.slint declare
 *   globals; the only writable globals are the window's inputs to Tokens
 *   (its width, height and face); only the bindings (main.slint, and the
 *   previews that stand in for it) write a global or read those inputs;
 *   everything else reads derived values only, and a control's place
 *   reaches a scroll region through properties and callbacks, never a
 *   global; and only main.slint and previews make windows.
 */
import { describe, expect, test } from "bun:test"
import { basename, dirname, join } from "node:path"
import { ADDED, FOLDER, LAYERS, type Layer, MERGED, PICO_TREE, SUFFIX } from "./inventory"
import { code, components, exists, imports, read, rel, ROOT, slintFiles, stringsBlanked } from "./slint-source"

/** A global's pure functions (they compute, they do not write). */
function isPure(global: string, member: string): boolean {
  const file = global === "P8" ? "ui/palette.slint" : "ui/tokens.slint"
  return new RegExp(`public\\s+pure\\s+function\\s+${member}\\b`).test(code(read(join(ROOT, file))))
}

const FOUNDATION = ["ui/tokens.slint", "ui/palette.slint"]
const rank = (path: string): number => {
  const at = rel(path)
  if (FOUNDATION.includes(at)) return -1
  const layer = LAYERS.findIndex(candidate => at.startsWith(`${FOLDER[candidate]}/`))
  return layer
}
const isPreview = (path: string) => path.endsWith(".preview.slint")
const layered = slintFiles().filter(path => rank(path) >= 0)
const componentFiles = layered.filter(path => !isPreview(path))

describe("the React tree, one Slint component each", () => {
  for (const layer of LAYERS) {
    for (const name of PICO_TREE[layer]) {
      test(`${layer}/${name}`, () => {
        if (MERGED[name] !== undefined) return
        const file = join(ROOT, FOLDER[layer], `${name}.slint`)
        expect(exists(file) ? rel(file) : `missing ${rel(file)}`).toBe(rel(file))
        expect(components(read(file))).toEqual([name])
      })
    }
  }
})

describe("every component file", () => {
  test("declares one component, named as its file, from the tree or ADDED", () => {
    const wrong = componentFiles.flatMap(path => {
      const name = basename(path, ".slint")
      const declared = components(read(path))
      const layer = LAYERS[rank(path)] as Layer
      const known = PICO_TREE[layer].includes(name) || ADDED[name]?.layer === layer
      return declared.length === 1 && declared[0] === name && known ? [] : [`${rel(path)}: ${declared.join(", ") || "none"}${known ? "" : " (not in the tree)"}`]
    })
    expect(wrong).toEqual([])
  })

  test("has a preview beside it that imports and draws it", () => {
    const missing = componentFiles.flatMap(path => {
      const name = basename(path, ".slint")
      const layer = LAYERS[rank(path)] as Layer
      const preview = join(dirname(path), `${name}.${SUFFIX[layer]}.preview.slint`)
      if (!exists(preview)) return [`${rel(path)}: no ${basename(preview)}`]
      const text = code(read(preview))
      const imported = imports(preview).some(found => found.from === path && found.names.includes(name))
      const drawn = new RegExp(`\\b${name}\\s*\\{`).test(text)
      const exported = /export\s+component\s+\w+/.test(text)
      return imported && drawn && exported ? [] : [`${rel(preview)}: ${imported ? "" : "does not import it "}${drawn ? "" : "does not draw it "}${exported ? "" : "exports no state"}`]
    })
    expect(missing).toEqual([])
  })

  test("has no preview without a component", () => {
    const orphans = layered.filter(isPreview).filter(path => {
      const name = basename(path).split(".")[0] ?? ""
      return !exists(join(dirname(path), `${name}.slint`))
    })
    expect(orphans.map(rel)).toEqual([])
  })
})

describe("imports point down", () => {
  test("no component imports a higher layer or main.slint", () => {
    const up = layered.flatMap(path =>
      imports(path).flatMap(found => {
        if (found.from.endsWith(".ttf")) return []
        if (found.from === join(ROOT, "main.slint")) return [`${rel(path)} imports main.slint`]
        const to = rank(found.from)
        if (to === -1 && !isPreview(path) && rank(path) >= 0) return []
        if (to < 0 && !FOUNDATION.includes(rel(found.from))) return [`${rel(path)} imports ${rel(found.from)}`]
        return to > rank(path) ? [`${rel(path)} imports up into ${rel(found.from)}`] : []
      }))
    expect(up).toEqual([])
  })
})

describe("shapes read no state", () => {
  test("only the tokens and the palette declare globals", () => {
    const globals = slintFiles().filter(path => !FOUNDATION.includes(rel(path)) && /(^|\n)\s*(export\s+)?global\s+\w+/.test(code(read(path))))
    expect(globals.map(rel)).toEqual([])
  })

  /** The window's inputs: the only writable global state, set by the bindings. */
  const INPUTS = ["Tokens.width", "Tokens.height", "Tokens.rows", "Tokens.family"]
  const isBinding = (path: string) => rel(path) === "main.slint" || isPreview(path) || FOUNDATION.includes(rel(path))
  const writable = FOUNDATION.flatMap(file => {
    const text = code(read(join(ROOT, file)))
    return [...text.matchAll(/global\s+(\w+)\s*\{([\s\S]*?)\n\}/g)].flatMap(([, name, body]) =>
      [...(body ?? "").matchAll(/in-out\s+property\s*<[^>]+>\s*([\w-]+)/g)].map(match => `${name}.${match[1]}`))
  })

  test("the only writable globals are the window's inputs", () => {
    expect(writable.filter(name => !INPUTS.includes(name))).toEqual([])
  })

  test("no file but the bindings writes a global or reads its inputs", () => {
    const globals = [...new Set(writable.map(name => name.split(".")[0]))]
    const found = slintFiles().filter(path => !isBinding(path)).flatMap(path => {
      const text = stringsBlanked(read(path))
      const writes = [...text.matchAll(/\b(\w+)\.([\w-]+)\s*(?:=(?!=)|\+=|-=|\()/g)]
        .filter(([, global, member]) => ["P8", "Tokens", "Cycle", ...globals].includes(global ?? "") && !isPure(global ?? "", member ?? ""))
        .map(([whole]) => `${rel(path)} writes ${whole}`)
      const reads = writable.filter(name => new RegExp(`\\b${name.replace(".", "\\.")}\\b`).test(text)).map(name => `${rel(path)} reads ${name}`)
      return [...writes, ...reads]
    })
    expect(found).toEqual([])
  })

  test("only main.slint and previews make windows", () => {
    const windows = slintFiles().filter(path => rel(path) !== "main.slint" && !isPreview(path) && /inherits\s+Window\b/.test(code(read(path))))
    expect(windows.map(rel)).toEqual([])
  })
})
