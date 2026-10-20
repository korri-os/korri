/**
 * Zone's atomic gate: three rules the other gates do not hold.
 *
 *  1. Shapes read no state. Nothing under src/ui/ or src/pages/ imports from
 *     src/state/ or reads the host. A shape takes plain props and reports
 *     through callbacks; the binding (ZoneSurface.tsx) is the only reader.
 *  2. Raw colour and pixel values live in src/zone-tokens.css and nowhere
 *     else. `0` is not a value. `1px` is allowed only where a token declares
 *     it. The font files and zone-fonts.css, which names the TTF, are exempt.
 *  3. Every component file declares exactly one component: not zero, which is
 *     a file in the wrong folder, and not two, which is a unit nobody named.
 */
import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync, statSync } from "node:fs"
import { basename, dirname, join, relative } from "node:path"
import { ResolverFactory } from "oxc-resolver"
import { parseModule, visitNodes } from "./source-ast"

const SRC = join(import.meta.dir, "..", "src")
const TOKENS = "zone-tokens.css"
const FONTS_CSS = "zone-fonts.css"

function walk(dir: string): readonly string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry)
    return statSync(full).isDirectory() ? walk(full) : [full]
  })
}

const allFiles = walk(SRC)
const read = (file: string) => readFileSync(file, "utf8")
const rel = (file: string) => relative(SRC, file)
const shapeFiles = allFiles.filter((file) =>
  /\.tsx?$/.test(file) && (rel(file).startsWith("ui/") || rel(file).startsWith("pages/")))
const componentFiles = shapeFiles.filter((file) =>
  file.endsWith(".tsx") && !file.endsWith(".part.tsx"))

describe("shapes read no state", () => {
  const resolver = new ResolverFactory({
    tsconfig: { configFile: join(SRC, "..", "tsconfig.json") },
    extensions: [".tsx", ".ts", ".js"],
    extensionAlias: { ".js": [".tsx", ".ts", ".js"] },
  })

  test("no file under src/ui or src/pages imports from src/state", () => {
    const offenders = shapeFiles.flatMap((file) => {
      const program = parseModule(file, read(file))
      const specifiers: string[] = []
      visitNodes(program, (node) => {
        if (node.type === "ImportDeclaration") specifiers.push(node.source.value)
        if ((node.type === "ExportNamedDeclaration" || node.type === "ExportAllDeclaration") && node.source) {
          specifiers.push(node.source.value)
        }
      })
      return specifiers.flatMap((specifier) => {
        if (!specifier.startsWith(".")) return []
        const target = resolver.sync(dirname(file), specifier).path
        return target !== undefined && rel(target).startsWith("state/")
          ? [`${rel(file)} -> ${specifier}`] : []
      })
    })
    expect(offenders.sort()).toEqual([])
  })

  /**
   * A shape that takes the host, even as a type, can call it. A shape that
   * names `host` has one. The treaty's model types stay allowed: a shape may
   * take a `SurfaceGame` as a prop.
   */
  test("no file under src/ui or src/pages reads the host", () => {
    const offenders = shapeFiles.flatMap((file) => {
      const source = read(file)
      const found: string[] = []
      if (/\bSurfaceHost\b|\bSurfaceInput\b/.test(source)) found.push("imports the host type")
      if (/\bhost\s*[.:]/.test(source)) found.push("reads host")
      return found.map((what) => `${rel(file)}: ${what}`)
    })
    expect(offenders.sort()).toEqual([])
  })
})

describe("raw values live in the tokens", () => {
  /**
   * A hex colour, an rgb()/hsl() colour, or a length in px. `0` carries no
   * unit and is not a value. `1px` outside the tokens is a hairline nobody
   * named: declare it as a token and use `var()`.
   */
  const RAW_VALUE = /#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?)\(|(?<![\w.-])\d*\.?\d+px\b/g

  const checked = allFiles.filter((file) =>
    (/\.(?:css|tsx?)$/.test(file))
    && basename(file) !== TOKENS
    && basename(file) !== FONTS_CSS
    && !rel(file).startsWith("fonts/"))

  test("no file outside zone-tokens.css states a raw colour or pixel value", () => {
    const offenders = checked.flatMap((file) => {
      const source = read(file).replace(/\/\*[\s\S]*?\*\//g, "")
      return [...source.matchAll(RAW_VALUE)].map((match) => `${rel(file)}: ${match[0]}`)
    })
    expect(offenders.sort()).toEqual([])
  })

  test("every token value is a colour, a length, a number or a keyword, never a formula", () => {
    const source = read(join(SRC, TOKENS)).replace(/\/\*[\s\S]*?\*\//g, "")
    const offenders = [...source.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)]
      .filter(([, , value]) => /\b(?:calc|clamp|min|max)\(/.test(value ?? ""))
      .map(([, name]) => name ?? "")
    expect(offenders).toEqual([])
  })

  /**
   * Three workers append their token blocks to the one file, and a merge once
   * dropped a closing brace: the next block nested inside the previous rule,
   * the parser threw it away, and every dialog rendered as bare text. Every
   * declaration must sit directly inside a `.zone-theme` or `:root` rule.
   */
  test("every token block closes before the next opens, so no block is lost", () => {
    const source = read(join(SRC, TOKENS)).replace(/\/\*[\s\S]*?\*\//g, "")
    let depth = 0
    const problems: string[] = []
    source.split("\n").forEach((line, index) => {
      const opens = (line.match(/\{/g) ?? []).length
      const closes = (line.match(/\}/g) ?? []).length
      if (opens > 0 && depth > 0) problems.push(`line ${index + 1}: a rule opens inside another`)
      depth += opens - closes
      if (depth < 0) problems.push(`line ${index + 1}: a rule closes that never opened`)
      if (depth === 0 && /^\s*--[a-z0-9-]+\s*:/.test(line)) problems.push(`line ${index + 1}: a token outside any rule`)
    })
    if (depth !== 0) problems.push(`end of file: ${depth} rule(s) never close`)
    expect(problems).toEqual([])
  })
})

describe("every component file holds exactly one component", () => {
  /**
   * Counted on the tree, not by regex: a top-level function declaration or a
   * `const` bound to a function, with a PascalCase name. SCREAMING_CASE is a
   * constant, lowercase is a helper.
   */
  function componentNames(file: string): string[] {
    const program = parseModule(file, read(file))
    const names: string[] = []
    const isComponentName = (name: string) => /^[A-Z][a-z0-9][A-Za-z0-9]*$/.test(name)
    for (const statement of program.body) {
      const declaration = statement.type === "ExportNamedDeclaration" ? statement.declaration
        : statement.type === "ExportDefaultDeclaration" ? statement.declaration
        : statement
      if (declaration === null || declaration === undefined) continue
      if (declaration.type === "FunctionDeclaration" && declaration.id && isComponentName(declaration.id.name)) {
        names.push(declaration.id.name)
      }
      if (declaration.type === "VariableDeclaration") {
        for (const declarator of declaration.declarations) {
          if (declarator.id.type === "Identifier" && isComponentName(declarator.id.name)
            && (declarator.init?.type === "ArrowFunctionExpression" || declarator.init?.type === "FunctionExpression")) {
            names.push(declarator.id.name)
          }
        }
      }
    }
    return names
  }

  test("no component file declares zero or two components", () => {
    const offenders = componentFiles
      .map((file) => ({ file: rel(file), names: componentNames(file) }))
      .filter(({ names }) => names.length !== 1)
      .map(({ file, names }) => `${file}: ${names.length === 0 ? "none" : names.join(", ")}`)
    expect(offenders.sort()).toEqual([])
  })
})
