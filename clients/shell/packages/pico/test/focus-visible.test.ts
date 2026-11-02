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
 *
 * Each render costs about a second, so the walk is planned first on the
 * logic alone (walk-plan.ts). Each model walks only as far as its last
 * focus target that no earlier model reached. A model that reaches nothing
 * new is not walked. The walks then run side by side, one probe per model
 * and size.
 */
import { expect, test } from "bun:test"
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs"
import { availableParallelism, tmpdir } from "node:os"
import { join } from "node:path"
import { FIVE_SIZES, PROBE } from "./probe"
import { ROOT } from "./slint-source"
import { walkStates } from "./walk-plan"

const MODELS = [
  "home-ready", "home-playing-now", "home-downloading", "home-problem", "home-error", "store-results", "store-entry",
  "store-providers", "launch-default", "runner-ready", "setup-hello", "setup-wifi", "setup-pads", "overlay-default",
  "settings-wifi", "settings-identity", "settings-plugins",
  // Korri reports a display larger than the window: Pico must fit the window.
  "home-big-display",
]

/** How far the full walk goes, per model; the plan keeps every target it reaches. */
const REFERENCE = Number(process.env["PICO_FOCUS_STATES"] ?? "80")

/** Write the scenario models once, and the large-display variant. */
function models(): string {
  const gen = mkdtempSync(join(tmpdir(), "pico-models-"))
  const write = Bun.spawnSync(["bun", join(ROOT, "preview", "scenarios.ts"), gen], { cwd: ROOT })
  expect(write.exitCode).toBe(0)
  const home = JSON.parse(readFileSync(join(gen, "home-ready.json"), "utf8"))
  home.displays = [{ id: "big", main: true, x: 0, y: 0, width: 1920, height: 1080, touch: false }]
  writeFileSync(join(gen, "home-big-display.json"), JSON.stringify(home))
  return gen
}

interface Planned {
  readonly name: string
  readonly states: number
  readonly targets: readonly string[]
}

/** Each model's walk: as far as its last new focus target. */
function plan(gen: string): Planned[] {
  const reached = new Set<string>()
  const planned: Planned[] = []
  for (const name of MODELS) {
    const file = JSON.parse(readFileSync(join(gen, `${name}.json`), "utf8"))
    // The large display changes the logic's plan, so its targets are its own.
    const known = name === "home-big-display" ? new Set<string>() : reached
    let states = 0
    const targets: string[] = []
    walkStates(file, REFERENCE).forEach((state, i) => {
      if (state.target !== undefined && !known.has(state.target)) {
        known.add(state.target)
        targets.push(state.target)
        states = i + 1
      }
    })
    if (states > 0) planned.push({ name, states, targets })
  }
  return planned
}

/** Run the walks side by side; the probe mostly waits for frames to settle. */
async function walks(gen: string, planned: readonly Planned[]): Promise<string[]> {
  const jobs = planned.flatMap(walk => FIVE_SIZES.split(",").map(size => ({ ...walk, size })))
  const failures: string[] = []
  const width = Math.max(1, Number(process.env["PICO_WALK_JOBS"] ?? Math.min(16, availableParallelism() * 2)))
  let next = 0
  const worker = async () => {
    while (next < jobs.length) {
      const job = jobs[next++]
      if (job === undefined) return
      const out = join(gen, "unseen", `${job.name}-${job.size}`)
      const run = Bun.spawn(
        [PROBE as string, "walk", "pico", "--model", join(gen, `${job.name}.json`), "--sizes", job.size, "--max", String(job.states), "--out", out],
        { cwd: join(ROOT, ".."), stdout: "pipe", stderr: "pipe" },
      )
      const text = `${await new Response(run.stdout).text()}${await new Response(run.stderr).text()}`
      const code = await run.exited
      failures.push(...text.split("\n").filter(line => line.startsWith("FAIL")))
      if (code !== 0 && !text.includes("FAIL")) failures.push(`${job.name} ${job.size}: probe exited ${code}: ${text.slice(-400)}`)
    }
  }
  await Promise.all(Array.from({ length: width }, worker))
  return failures
}

test("the plan keeps MENU's list, with and without GET GAMES, and the large display", () => {
  const planned = plan(models())
  const targets = planned.flatMap(walk => walk.targets)
  // Down puts the cursor on MENU; confirm opens the list. The store's
  // providers add GET GAMES, one entry more.
  expect(targets.filter(target => /^home:[^|]*:menu\|\.home\.menu\.\d+$/.test(target)).length).toBeGreaterThanOrEqual(4)
  expect(planned.find(walk => walk.name === "home-big-display")).toBeDefined()
  // The plan walks far fewer states than the full walk.
  expect(planned.reduce((sum, walk) => sum + walk.states, 0)).toBeLessThan(MODELS.length * REFERENCE)
}, 120_000)

test.skipIf(PROBE === undefined)("every focus target the logic can reach is inside the window, under any display", async () => {
  const gen = models()
  expect(await walks(gen, plan(gen))).toEqual([])
}, 3_600_000)
