// Build-machine experiment only. Policy resolution uses the plugin-local frozen
// install; optional platform sources use the separate probe-local frozen install.
// No files from this probe or its output are shipped to a plugin runtime.
import ts from "../../../plugins/retroarch/node_modules/typescript/lib/typescript.js"
import { readFileSync, mkdirSync, writeFileSync, realpathSync } from "node:fs"
import { resolve, dirname, relative } from "node:path"
import { createHash } from "node:crypto"
import { primitiveCases } from "./primitive-cases"

const [mode, output, platform = "none"] = process.argv.slice(2)
if (!["direct", "schema", "bundle", "bundle-minify", "schema-minify", "esbuild"].includes(mode) || !output || !["none", "real"].includes(platform)) {
  throw new Error("usage: bun prepare.ts direct|schema|bundle|bundle-minify|schema-minify|esbuild OUTPUT.json [none|real]")
}
const start = performance.now()
const root = resolve(import.meta.dir, "../../..")
const plugin = resolve(root, "plugins/retroarch")
const modules: Record<string, { source: string; dependencies: Record<string, string> }> = {}
const hashes: Record<string, string> = {}
const id = (path: string) => relative(root, path)
const entry = "probe-entry.js"
const wrapper = `import { decodeRetroArchPolicy } from "./plugins/retroarch/policy.ts";
import { renderRetroArchSettings } from "./plugins/retroarch/render-settings.ts";
import { isSchemaError } from "effect/Schema";
export function probe(input) {
  try { return JSON.stringify({ ok: true, pairs: renderRetroArchSettings(decodeRetroArchPolicy(input)) }); }
  catch (error) { return JSON.stringify({ ok: false, errorKind: isSchemaError(error) ? "SchemaError" : error.name, error: String(error), stack: error.stack }); }
}`
function visit(path: string): string {
  path = realpathSync(path)
  if (!path.startsWith(plugin + "/")) throw new Error(`Outside approved experimental source graph: ${path}`)
  const name = id(path)
  if (modules[name]) return name
  let source = readFileSync(path, "utf8")
  hashes[name] = createHash("sha256").update(source).digest("hex")
  // Separate public-subpath experiment; the repository policy and vendor source stay unchanged.
  if (mode.startsWith("schema") && path === resolve(plugin, "policy.ts")) {
    source = source.replace('import { Schema } from "effect"', 'import * as Schema from "effect/Schema"')
  }
  const dependencies: Record<string, string> = {}
  modules[name] = { source, dependencies }
  const ast = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true)
  for (const statement of ast.statements) {
    if (!ts.isImportDeclaration(statement) && !ts.isExportDeclaration(statement)) continue
    if (!statement.moduleSpecifier || !ts.isStringLiteral(statement.moduleSpecifier)) continue
    if (ts.isImportDeclaration(statement) && statement.importClause?.isTypeOnly) continue
    if (ts.isExportDeclaration(statement) && statement.isTypeOnly) continue
    const specifier = statement.moduleSpecifier.text
    dependencies[specifier] = visit(Bun.resolveSync(specifier, dirname(path)))
  }
  return name
}
const policy = visit(resolve(plugin, "policy.ts"))
const renderer = visit(resolve(plugin, "render-settings.ts"))
modules[entry] = { source: wrapper, dependencies: {
  "./plugins/retroarch/policy.ts": policy,
  "./plugins/retroarch/render-settings.ts": renderer,
  "effect/Schema": visit(Bun.resolveSync("effect/Schema", plugin)),
} }
const graphMs = performance.now() - start
const sourceBytes = Object.values(modules).reduce((n, m) => n + Buffer.byteLength(m.source), 0)
const graphModules = Object.keys(modules).length
let bundleMs: number | null = null
if (mode.startsWith("bundle") || mode === "schema-minify") {
  // Experimental external bundler, not korrid's current Oxc transpilation path.
  // Virtual inputs come from the measured graph (with the public-subpath import
  // when requested); no vendor source or policy decoder body is changed.
  const bundleStart = performance.now()
  const build = await Bun.build({
    entrypoints: ["probe-source:" + entry], target: "browser", format: "esm",
    minify: mode.endsWith("minify"), sourcemap: "none", plugins: [{ name: "approved-memory-graph", setup(build) {
      build.onResolve({ filter: /^probe-source:/ }, ({ path }) => ({ path: path.slice(13), namespace: "approved" }))
      build.onResolve({ filter: /.*/, namespace: "approved" }, ({ path, importer }) => {
        const target = modules[importer]?.dependencies[path]
        if (!target) throw new Error(`Unapproved import ${path} from ${importer}`)
        return { path: target, namespace: "approved" }
      })
      build.onLoad({ filter: /.*/, namespace: "approved" }, ({ path }) => ({
        contents: modules[path].source, loader: path.endsWith(".ts") ? "ts" : "js",
      }))
    } }],
  })
  if (!build.success) throw new Error(build.logs.map(String).join("\n"))
  const source = await build.outputs[0].text()
  bundleMs = performance.now() - bundleStart
  for (const key of Object.keys(modules)) delete modules[key]
  modules[entry] = { source, dependencies: {} }
}
if (mode === "esbuild") {
  const bundleStart = performance.now()
  const executable = process.env.PROBE_ESBUILD
  if (!executable) throw new Error("PROBE_ESBUILD must name esbuild 0.25.12")
  const version = Bun.spawnSync([executable, "--version"])
  if (version.stdout.toString().trim() !== "0.25.12") throw new Error("Wrong esbuild version")
  const build = Bun.spawnSync([executable, "--bundle", "--format=esm", "--platform=neutral", "--target=es2022", "--tree-shaking=true"], {
    cwd: root, stdin: Buffer.from(wrapper),
  })
  if (build.exitCode !== 0) throw new Error(build.stderr.toString())
  const source = build.stdout.toString()
  bundleMs = performance.now() - bundleStart
  for (const key of Object.keys(modules)) delete modules[key]
  modules[entry] = { source, dependencies: {} }
}
// Real source libraries are separate, ordered modules: URL's module scope needs
// TextEncoder/TextDecoder already installed. No vendor edits or host bindings.
const platformModules: { name: string; source: string }[] = []
const platformPreparation = performance.now()
const platformHashes: Record<string, string> = {}
if (platform === "real") {
  const approvedRoot = realpathSync(import.meta.dir) + "/"
  for (const name of ["text-platform", "url-platform"]) {
    const build = await Bun.build({
      entrypoints: [resolve(import.meta.dir, `${name}.ts`)],
      target: "browser", format: "esm", minify: true, sourcemap: "none",
      plugins: [{ name: "probe-only-platform-sources", setup(build) {
        build.onResolve({ filter: /^(node:|bun:)/ }, ({ path }) => { throw new Error(`Host API import forbidden: ${path}`) })
        build.onLoad({ filter: /\.(js|ts|json)$/ }, ({ path }) => {
          const actual = realpathSync(path)
          if (!actual.startsWith(approvedRoot)) throw new Error(`Outside probe platform graph: ${actual}`)
          const source = readFileSync(actual, "utf8")
          platformHashes[id(actual)] = createHash("sha256").update(source).digest("hex")
          return { contents: source, loader: actual.endsWith(".json") ? "json" : actual.endsWith(".ts") ? "ts" : "js" }
        })
      } }],
    })
    if (!build.success) throw new Error(build.logs.map(String).join("\n"))
    platformModules.push({ name: `${name}.js`, source: await build.outputs[0].text() })
  }
}
const platformPreparationMs = performance.now() - platformPreparation
const primitiveSource = readFileSync(resolve(import.meta.dir, "primitive-cases.ts"), "utf8")
const nativeControlStart = performance.now()
const primitiveExpected = primitiveCases()
const nativePrimitiveControlMs = performance.now() - nativeControlStart
const result = { entry, modules, platform_modules: platformModules,
  primitive_source: primitiveSource, primitive_expected: primitiveExpected, metadata: {
  native_primitive_control_ms: nativePrimitiveControlMs,
  mode, platform, graph_modules: graphModules, graph_source_bytes: sourceBytes,
  platform_preparation_ms: platformPreparationMs,
  platform_js_bytes: platformModules.reduce((n, m) => n + Buffer.byteLength(m.source), 0),
  platform_source_sha256: platformHashes,
  platform_versions: platform === "real" ? Object.fromEntries(["@kayahr/text-encoding", "whatwg-url", "@exodus/bytes", "webidl-conversions", "tr46", "punycode"].map(name =>
    [name, JSON.parse(readFileSync(resolve(import.meta.dir, "node_modules", name, "package.json"), "utf8")).version])) : {},
  primitive_source_sha256: createHash("sha256").update(primitiveSource).digest("hex"),
  graph_preparation_ms: graphMs, experimental_bundle_ms: bundleMs,
  bundler: mode === "esbuild" ? "esbuild 0.25.12" : mode.startsWith("bundle") || mode === "schema-minify" ? `Bun ${Bun.version}` : null,
  prepared_source_bytes: Object.values(modules).reduce((n, m) => n + Buffer.byteLength(m.source), 0),
  total_preparation_ms: performance.now() - start,
  versions: Object.fromEntries(["effect", "fast-check", "pure-rand", "typescript"].map(name =>
    [name, JSON.parse(readFileSync(resolve(plugin, "node_modules", name, "package.json"), "utf8")).version])),
  bun: Bun.version, original_source_sha256: hashes,
} }
mkdirSync(dirname(resolve(output)), { recursive: true })
writeFileSync(output, JSON.stringify(result))
console.log(JSON.stringify({ ...result.metadata, original_source_sha256: "in manifest", platform_source_sha256: "in manifest" }))
