/**
 * Nothing the cursor can reach is out of sight, at any of Pico's five sizes.
 *
 * The probe runs logic.js on a fixture model, answers MoveCatalog and
 * FocusGame as korrid does, and presses every button from every state it
 * reaches (breadth first, up to a cap). At each state with a focused
 * control it renders the view as it is and with every `focused` flag
 * cleared: the two differ only if the focused control is drawn inside the
 * window. A control scrolled away, clipped or covered fails.
 *
 * The scenarios are preview/scenarios.ts's models: home and its layouts,
 * Find, a game's screen, Settings and its questions, the store, launch
 * settings, the runner picker, setup and the pause menu.
 */
import { expect, test } from "bun:test"
import { mkdtempSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { FIVE_SIZES, PROBE, probe } from "./probe"
import { ROOT } from "./slint-source"

const MODELS = [
  "home-ready", "home-playing-now", "home-downloading", "home-problem", "home-error", "store-results", "store-entry",
  "launch-default", "runner-ready", "setup-hello", "setup-wifi", "setup-pads", "overlay-default", "settings-wifi",
  "settings-identity", "settings-plugins",
]

/** Write the scenario models once. */
function models(): string {
  const gen = mkdtempSync(join(tmpdir(), "pico-models-"))
  const write = Bun.spawnSync(["bun", join(ROOT, "preview", "scenarios.ts"), gen], { cwd: ROOT })
  expect(write.exitCode).toBe(0)
  return gen
}

function walk(gen: string, names: readonly string[], extra: readonly string[] = []) {
  const max = process.env["PICO_FOCUS_STATES"] ?? "80"
  const run = probe(["walk", "pico", "--model", names.map(name => join(gen, `${name}.json`)).join(","), "--sizes", FIVE_SIZES, "--max", max, "--out", join(gen, "unseen"), ...extra])
  expect(run.out.split("\n").filter(line => line.startsWith("FAIL"))).toEqual([])
  expect(run.code).toBe(0)
}

test.skipIf(PROBE === undefined)("every focus target the logic can reach is inside the window", () => {
  walk(models(), MODELS)
}, 7_200_000)

test.skipIf(PROBE === undefined)("MENU's list, with and without GET GAMES, is inside the window", () => {
  // Down puts the cursor on MENU; confirm opens the list.
  walk(models(), ["home-ready", "store-providers"], ["--start", "down,confirm"])
}, 3_600_000)

test.skipIf(PROBE === undefined)("the grid fits the window, not a larger display", () => {
  // Korri reports a 1920x1080 display; the window is each of the five sizes.
  walk(models(), ["home-ready"], ["--start", "menu", "--display", "1920x1080"])
}, 3_600_000)
