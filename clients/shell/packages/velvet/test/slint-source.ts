/**
 * Reading the package's .slint sources for the gates: which files are
 * components, which are previews, what each declares and imports. Comments
 * are stripped first, so a word in prose never counts as code.
 */
import { readdirSync, readFileSync, statSync } from "node:fs"
import { join, relative } from "node:path"

export const PACKAGE = join(import.meta.dir, "..")

/** The layers, lowest first; a file may import only its own layer or lower. */
export const LAYERS = ["atoms", "molecules", "organisms", "templates", "pages"] as const
export type Layer = (typeof LAYERS)[number]
export const LAYER_SUFFIX: Readonly<Record<Layer, string>> = {
  atoms: "atom", molecules: "molecule", organisms: "organism", templates: "template", pages: "page",
}

/** The shared definitions every layer may import: the tokens global, the view's struct types, and the no-art cards' drawings (data, written by drawings/build.ts). */
export const FOUNDATION = ["ui/tokens.slint", "ui/types.slint", "ui/drawings.slint"] as const

export const stripComments = (source: string): string =>
  source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "")

function walk(dir: string, out: string[]): void {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) walk(path, out)
    else if (name.endsWith(".slint")) out.push(path)
  }
}

/** Every .slint file in the package, relative to it, sorted. */
export function slintFiles(root = PACKAGE): string[] {
  const out: string[] = []
  for (const top of ["ui", "pages"]) {
    try {
      walk(join(root, top), out)
    } catch {
      // A missing folder is a gate failure, reported by the gates themselves.
    }
  }
  out.push(join(root, "main.slint"))
  return out.map(path => relative(root, path)).sort()
}

export const isPreview = (file: string): boolean => file.endsWith(".preview.slint")

/** The layer a file stands in, from its folder; undefined for main.slint and the foundation. */
export function layerOf(file: string): Layer | undefined {
  if (file.startsWith("pages/")) return "pages"
  const match = /^ui\/(atoms|molecules|organisms|templates)\//.exec(file)
  return match === null ? undefined : (match[1] as Layer)
}

export const read = (file: string, root = PACKAGE): string => readFileSync(join(root, file), "utf8")

/** The components a file declares (exported or not). */
export const componentsIn = (source: string): string[] =>
  [...stripComments(source).matchAll(/^\s*(?:export\s+)?component\s+([A-Za-z][\w-]*)/gm)].map(match => match[1]!)

/** The files a file imports, resolved relative to the package. */
export function importsOf(file: string, source: string): string[] {
  const dir = file.includes("/") ? file.slice(0, file.lastIndexOf("/")) : ""
  return [...stripComments(source).matchAll(/import\s*(?:\{[^}]*\}\s*from\s*)?"([^"]+\.slint)"/g)].map(match => {
    const parts = (dir === "" ? [] : dir.split("/"))
    for (const piece of match[1]!.split("/")) {
      if (piece === "..") parts.pop()
      else if (piece !== ".") parts.push(piece)
    }
    return parts.join("/")
  })
}
