/**
 * The portal ships no outside plugin repository address (ADR 0042). A device
 * starts with the official repository its configuration names, or none; any
 * other repository is one its owner typed.
 *
 * This walks every Korri source file the portal's entry reaches, the default
 * surface included, the way the bundler does (static and dynamic imports,
 * the `@contracts` and `@korri/<surface>` aliases), and reads every http(s)
 * address in them. Each must name a host reserved for examples (RFC 2606 and
 * RFC 6761), this device, or an address listed below as not a plugin
 * repository. Third-party packages are not walked: they are not Korri's, and
 * name no Korri repository.
 */
import { describe, expect, it } from "bun:test"
import { existsSync, readFileSync, statSync } from "node:fs"
import { dirname, join, relative, resolve } from "node:path"
import { pluginSettingGroups, initialPluginFacts } from "./surface/plugin-repositories"
import { DEFAULT_SURFACE_ID } from "./surface/surface-registry"

const PORTAL = resolve(import.meta.dir, "..")
const ROOT = resolve(PORTAL, "../..")
const ENTRY = join(PORTAL, "src/main.tsx")

/** The bundler's aliases (vite.config.ts and tsconfig.json `paths`). */
const ALIASES: readonly (readonly [string, string])[] = [
  ["@contracts/", join(ROOT, "contracts/")],
  ["@korri/pico", join(ROOT, "surfaces/pico/src/index.ts")],
  ["@korri/zone", join(ROOT, "surfaces/zone/src/index.ts")],
  ["@korri/boxbuster", join(ROOT, "surfaces/boxbuster/src/index.ts")],
]

/** Addresses in shipped code that are not plugin repositories, and why. */
const NOT_REPOSITORIES: Readonly<Record<string, string>> = {
  "www.lexaloffle.com": "game pages of the PICO-8 BBS in the in-memory store sample (store-acquisition-client.ts)",
}

/** Hosts no one can publish a repository on. */
const reserved = (host: string): boolean =>
  host === "localhost" ||
  host === "127.0.0.1" ||
  /^example\.(com|net|org)$/.test(host) ||
  /\.(example|test|invalid|localhost)$/.test(host) ||
  host === "example"

const SOURCE = /\.(ts|tsx|js|mjs)$/
const CANDIDATES = ["", ".ts", ".tsx", ".js", ".mjs", "/index.ts", "/index.tsx"]

function resolveFile(base: string): string | undefined {
  for (const suffix of CANDIDATES) {
    const path = base + suffix
    if (existsSync(path) && statSync(path).isFile()) return path
  }
  return undefined
}

function resolveImport(specifier: string, from: string): string | undefined {
  const bare = specifier.replace(/[?#].*$/, "")
  if (bare.startsWith(".")) return resolveFile(resolve(dirname(from), bare))
  for (const [alias, target] of ALIASES) {
    if (bare === alias || (alias.endsWith("/") && bare.startsWith(alias))) {
      return resolveFile(alias.endsWith("/") ? join(target, bare.slice(alias.length)) : target)
    }
  }
  return undefined // a third-party package
}

/* Static, side-effect and dynamic imports, and re-exports. */
const IMPORT = /\b(?:import|export)\s[^'"`;]*?\bfrom\s*['"]([^'"]+)['"]|\bimport\s*['"]([^'"]+)['"]|\bimport\s*\(\s*['"]([^'"]+)['"]\s*\)/g

/** Bun's scanner; a file it refuses for a JSX warning is read by pattern. */
function importsOf(transpiler: Bun.Transpiler, code: string): string[] {
  try {
    return transpiler.scanImports(code).map(entry => entry.path)
  } catch {
    return [...code.matchAll(IMPORT)].map(match => match[1] ?? match[2] ?? match[3] ?? "").filter(Boolean)
  }
}

/** Every Korri file the entry reaches, as the bundler follows imports. */
function shippedFiles(): string[] {
  const transpiler = new Bun.Transpiler({ loader: "tsx" })
  const seen = new Set<string>()
  const queue = [ENTRY]
  while (queue.length > 0) {
    const file = queue.pop()!
    if (seen.has(file)) continue
    seen.add(file)
    if (!SOURCE.test(file)) continue // a stylesheet or an asset: read, not followed
    for (const path of importsOf(transpiler, readFileSync(file, "utf8"))) {
      const next = resolveImport(path, file)
      if (next !== undefined) queue.push(next)
    }
  }
  return [...seen]
}

const ADDRESS = /\bhttps?:\/\/([A-Za-z0-9.-]+)/g
const TEXT = /\.(ts|tsx|js|mjs|css|json|html|svg|txt)$/

describe("outside repository addresses", () => {
  const files = shippedFiles()

  it("walks the portal and the default surface", () => {
    const names = files.map(file => relative(ROOT, file))
    expect(names).toContain("clients/portal/src/korrid/client.ts")
    expect(names).toContain("clients/portal/src/surface/plugin-repositories.ts")
    expect(names.some(name => name.startsWith(`surfaces/${DEFAULT_SURFACE_ID}/src/`))).toBe(true)
    // Tests and fixtures are not part of what ships.
    expect(names.filter(name => /\.test\.|\/test\//.test(name))).toEqual([])
  })

  it("are built into neither the portal nor the default surface", () => {
    const outside: string[] = []
    for (const file of files) {
      if (!TEXT.test(file)) continue
      for (const [, host = ""] of readFileSync(file, "utf8").matchAll(ADDRESS)) {
        if (!reserved(host.toLowerCase()) && NOT_REPOSITORIES[host.toLowerCase()] === undefined) {
          outside.push(`${relative(ROOT, file)}: ${host}`)
        }
      }
    }
    expect(outside).toEqual([])
  })

  it("leave a device with no repository until its configuration or its owner adds one", () => {
    const groups = pluginSettingGroups({
      ...initialPluginFacts,
      repositories: { _tag: "Ready", value: { userAdded: [], repositoryInstalls: [], otherInstalls: [] } },
    }, [])
    const words = JSON.stringify(groups)
    expect(words.match(ADDRESS) ?? []).toEqual([])
  })
})
