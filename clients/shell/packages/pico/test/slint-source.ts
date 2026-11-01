/**
 * Reading the package's .slint files for the gates: comments stripped,
 * components, imports and preview files found by plain parsing. Slint has no
 * parser in TypeScript; the grammar the gates need is small and stable.
 */
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs"
import { dirname, join, relative, resolve } from "node:path"

export const ROOT = join(import.meta.dir, "..")

/** A file's text with comments and string contents blanked, so a word in a
 * comment or a label is never mistaken for code. Strings keep their quotes. */
export function code(text: string): string {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, match => match.replace(/[^\n]/g, " "))
    .replace(/\/\/[^\n]*/g, match => " ".repeat(match.length))
}

export function stringsBlanked(text: string): string {
  return code(text).replace(/"(?:[^"\\]|\\.)*"/g, match => `"${" ".repeat(match.length - 2)}"`)
}

export function slintFiles(dir = ROOT): string[] {
  const out: string[] = []
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules" || entry === "preview" || entry === "test" || entry === "src" || entry.startsWith(".")) continue
    const path = join(dir, entry)
    if (statSync(path).isDirectory()) out.push(...slintFiles(path))
    else if (entry.endsWith(".slint")) out.push(path)
  }
  return out.sort()
}

export const rel = (path: string) => relative(ROOT, path)
export const read = (path: string) => readFileSync(path, "utf8")

/** Every `component Name` declared in the file, exported or not. */
export function components(text: string): string[] {
  return [...code(text).matchAll(/(?:^|\n)\s*(?:export\s+)?component\s+([A-Za-z_][\w-]*)/g)].map(match => match[1] ?? "")
}

/** Every `import { … } from "path"` (and bare `import "path"`), resolved. */
export function imports(path: string): { readonly names: string[]; readonly from: string }[] {
  const text = code(read(path))
  const found: { names: string[]; from: string }[] = []
  for (const match of text.matchAll(/import\s*(?:\{([^}]*)\}\s*from\s*)?"([^"]+)"/g)) {
    const names = (match[1] ?? "").split(",").map(name => name.trim().split(/\s+as\s+/)[0] ?? "").filter(Boolean)
    found.push({ names, from: resolve(dirname(path), match[2] ?? "") })
  }
  return found
}

export const exists = existsSync
