/**
 * Pico's side of the focus convention (contract 8.4): what the shell's
 * probe reads, checked on the logic alone.
 *
 * - Every view entry with `focused` names its `focusRegion`.
 * - A region has at most one `focusActive` entry.
 * - Every exit in `focusPlan` leads to a region the view has.
 * - A press that leaves a region goes where `focusPlan` says, entering by
 *   its rule. The probe judges moves inside a region by what is drawn.
 */
import { describe, expect, test } from "bun:test"
import { mkdtempSync, readFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { ROOT } from "./slint-source"
import { session, walkStates } from "./walk-plan"
import { catalogScreen, undrawnTargets } from "../src/native/program"

type Json = Record<string, unknown>
type Exit = { readonly to: string; readonly entry: "remembered" | "active" | "first" }
type FocusPlan = Record<string, Partial<Record<string, Exit>>>

const MODELS = [
  "home-ready", "home-playing-now", "home-downloading", "home-problem", "home-error", "store-results", "store-entry",
  "store-providers", "launch-default", "launch-changes", "launch-unreachable", "launch-long", "runner-ready", "setup-hello", "setup-wifi", "setup-pads", "overlay-default", "overlay-stream",
  "settings-wifi", "settings-identity", "settings-plugins",
]
/** Spines above the rows, and beside them. */
const SIZES = [[320, 240], [1280, 720]] as const
const STATES = Number(process.env["PICO_CONVENTION_STATES"] ?? "60")
const DIRECTIONS = ["up", "down", "left", "right"] as const

interface Entry {
  readonly path: string
  readonly region: string | undefined
  readonly focused: boolean
  readonly active: boolean
}

function entries(value: unknown, path = "", into: Entry[] = []): Entry[] {
  if (Array.isArray(value)) value.forEach((item, i) => entries(item, `${path}.${i}`, into))
  else if (value !== null && typeof value === "object") {
    const entry = value as Json
    if ("focused" in entry) {
      into.push({
        path,
        region: typeof entry["focusRegion"] === "string" ? entry["focusRegion"] : undefined,
        focused: entry["focused"] === true,
        active: entry["focusActive"] === true,
      })
    }
    for (const [key, item] of Object.entries(entry)) if (key !== "focusPlan") entries(item, `${path}.${key}`, into)
  }
  return into
}

const gen = (() => {
  const dir = mkdtempSync(join(tmpdir(), "pico-convention-"))
  const write = Bun.spawnSync(["bun", join(ROOT, "preview", "scenarios.ts"), dir], { cwd: ROOT })
  if (write.exitCode !== 0) throw new Error(write.stderr.toString())
  return dir
})()

function viewAfter(file: Json, size: readonly [number, number], path: readonly string[]): Json {
  const pico = session(file)
  pico.resize(size[0], size[1])
  pico.deliver({ _tag: "KorriPublished" })
  pico.press(...path)
  return pico.view()
}

describe("the focus convention", () => {
  for (const name of MODELS) {
    test(`${name}: regions, active targets, exits`, () => {
      const file = JSON.parse(readFileSync(join(gen, `${name}.json`), "utf8")) as Json
      const problems: string[] = []
      for (const size of SIZES) {
        for (const state of walkStates(file, STATES, { width: size[0], height: size[1] })) {
          const at = `${size[0]}x${size[1]} [${state.path.join(" ")}]`
          const shown = viewAfter(file, size, state.path)
          {
            const pico = session(file)
            pico.resize(size[0], size[1])
            pico.deliver({ _tag: "KorriPublished" })
            pico.press(...state.path)
            if (file["presentation"] === undefined || (file["presentation"] as Json)["kind"] !== "gameplay-overlay") {
              const undrawn = undrawnTargets(catalogScreen(pico.state(), pico.model()))
              if (undrawn.length > 0) problems.push(`${at}: targets with no entry in the view: ${undrawn.join(", ")}`)
            }
          }
          const all = entries(shown)
          const plan = (shown["focusPlan"] ?? {}) as FocusPlan
          const regions = new Set(all.map(entry => entry.region))
          for (const entry of all) if (entry.region === undefined) problems.push(`${at}: ${entry.path} has no focusRegion`)
          for (const region of regions) {
            if (all.filter(entry => entry.region === region && entry.active).length > 1) problems.push(`${at}: ${region} has two active`)
          }
          for (const [region, exits] of Object.entries(plan)) {
            for (const exit of Object.values(exits)) {
              if (exit !== undefined && !regions.has(exit.to)) problems.push(`${at}: ${region} exits to ${exit.to}, not in the view`)
            }
          }
          const from = all.find(entry => entry.focused)
          if (from === undefined) continue
          for (const direction of DIRECTIONS) {
            const after = entries(viewAfter(file, size, [...state.path, direction]))
            const landed = after.find(entry => entry.focused)
            if (landed === undefined || landed.region === from.region) continue
            // A press that leaves the region's screen (a horizontal control,
            // a new page) is not a move: only moves within one view count.
            if (!after.some(entry => entry.path === from.path && entry.region === from.region)) continue
            const exit = plan[from.region ?? ""]?.[direction]
            if (exit === undefined) problems.push(`${at} ${direction}: left ${from.region} for ${landed.region} with no exit`)
            else if (exit.to !== landed.region) problems.push(`${at} ${direction}: ${from.region} exits to ${exit.to}, landed in ${landed.region}`)
            else if (exit.entry === "active" && after.some(entry => entry.region === exit.to && entry.active) && !landed.active) {
              problems.push(`${at} ${direction}: entered ${exit.to} off its active target`)
            }
          }
        }
      }
      expect(problems).toEqual([])
    }, 600_000)
  }
})
