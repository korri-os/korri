/**
 * Pico's state architecture is enforced by oxlint (.oxlintrc.json). A lint
 * rule nobody has seen fail proves nothing, so this test runs the real linter
 * over test/lint-tripwires, where every planted violation is marked
 * `// VIOLATION <rule>`, and requires exactly those reports: each marked line
 * reported for its rule, and nothing else. Then it requires src to be clean.
 */
import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync } from "node:fs"
import { join, relative } from "node:path"

const ROOT = join(import.meta.dir, "..")
const TRIPWIRES = join(import.meta.dir, "lint-tripwires")
const OXLINT = join(ROOT, "node_modules", ".bin", "oxlint")

interface Report {
  readonly code: string
  readonly filename: string
  readonly labels: readonly { readonly span: { readonly line: number } }[]
}

function lint(...paths: string[]): readonly Report[] {
  const run = Bun.spawnSync([OXLINT, "--type-aware", "--format", "json", ...paths], { cwd: ROOT })
  const output = run.stdout.toString()
  const parsed = JSON.parse(output.slice(output.indexOf("{"))) as { diagnostics: Report[] }
  return parsed.diagnostics
}

/** "file:line rule" for a report, with the rule name stripped of its plugin. */
const key = (file: string, line: number, rule: string) => `${file}:${line} ${rule}`

function walk(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name)
    return entry.isDirectory() ? walk(path) : [path]
  })
}

describe("the architecture rules", () => {
  test("report every planted violation and nothing else", () => {
    const expected = walk(TRIPWIRES).flatMap(file =>
      readFileSync(file, "utf8").split("\n").flatMap((text, index) => {
        const marked = text.match(/\/\/ VIOLATION ([a-z-]+)/)
        return marked ? [key(relative(ROOT, file), index + 1, marked[1]!)] : []
      }))
    const reported = lint(relative(ROOT, TRIPWIRES)).map(report =>
      key(report.filename, report.labels[0]?.span.line ?? 0, report.code.replace(/^[a-z]+\((.*)\)$/, "$1")))

    // Every rule has at least one tripwire.
    expect(new Set(expected.map(entry => entry.split(" ")[1]))).toEqual(new Set([
      "no-restricted-imports",
      "no-restricted-globals",
      "no-restricted-properties",
      "switch-exhaustiveness-check",
    ]))
    expect([...reported].sort()).toEqual([...expected].sort())
  })

  test("find nothing in src", () => {
    expect(lint("src").map(report => `${report.filename} ${report.code}`)).toEqual([])
  })
})
