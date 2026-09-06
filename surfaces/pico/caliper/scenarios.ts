import type { SurfaceModel } from "../../../contracts/surface/korri-surface"
import { fixtureModel, fixtureOverlay } from "../src/fixtures/fixture-host"

const base = { ...fixtureModel, buildLabel: "CALIPER FIXTURE" }
const overlay: SurfaceModel = { ...base, presentation: fixtureOverlay,
  status: { _tag: "Running", kicker: "PREVIEW: Game running", gameId: "hollow" } }
const problem = { _tag: "Problem", kicker: "PREVIEW: Launch failed",
  reason: "Simulated failure", canRetry: true, gameId: "hollow" } as const

/** Review models are the source picker: no separate state-name inventory. */
const models = {
  ready: base,
  loading: { ...base, catalog: { _tag: "Loading" } },
  empty: { ...base, catalog: { _tag: "Empty" } },
  "catalog-error": { ...base, catalog: { _tag: "Error", message: "PREVIEW: Library could not be read" } },
  busy: { ...base, status: { _tag: "Busy", kicker: "PREVIEW: Starting game", gameId: "hollow",
    detail: "Simulated request — no device action was performed." } },
  running: { ...base, status: overlay.status },
  problem: { ...base, status: problem },
  "settings-saving": { ...base, settingsStatus: { _tag: "Saving", settingId: "@korri:mgba" } },
  "settings-problem": { ...base, settingsStatus: { _tag: "Problem", settingId: "@korri:mgba", message: "PREVIEW: Setting was not saved" } },
  overlay,
  "overlay-problem": { ...overlay, status: { ...problem, kicker: "PREVIEW: Overlay command failed" } },
} satisfies Record<string, SurfaceModel>

export const scenarioOptions = Object.keys(models).map(id => ({ id, label: `Fixture: ${id}` }))
export function isScenario(value: unknown): value is keyof typeof models {
  return typeof value === "string" && Object.hasOwn(models, value)
}
export function modelForScenario(value: unknown, startsInOverlay = false): SurfaceModel {
  const id = isScenario(value) ? value : "ready"
  return id === "ready" && startsInOverlay ? overlay : models[id]
}
