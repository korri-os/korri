import type {
  SurfaceGame,
  SurfaceIdentityManagement,
  SurfaceIdentityStatus,
  SurfaceModel,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"
import { picoHomeViewFromCatalog } from "../pico-home-view"
import { picoSettingsViewFromModel } from "../pico-settings-view"
import { fixtureModel } from "./fixture-host"

/** Fail at the fixture lookup, rather than silently previewing a different game. */
export function fixtureGame(id: string): SurfaceGame {
  if (fixtureModel.catalog._tag !== "Ready") throw new Error("Expected the fixture catalog")
  const game = fixtureModel.catalog.games.find(game => game.id === id)
  if (game === undefined) throw new Error(`Missing fixture game: ${id}`)
  return game
}

// One missing-art input shared by the Home, shelf, cart and cover previews.
export const noArtworkGame = fixtureGame("petal")

// Content stress data uses existing treaty fields, not a new catalog schema.
export const longTitleGame: SurfaceGame = {
  ...fixtureGame("bramble"),
  title: "The Legend of Zelda: A Link to the Past & Four Swords",
  subtitle: "GBA · This device · Library on the removable storage card",
}

export function fixtureShelfGames(games: readonly SurfaceGame[]) {
  const view = picoHomeViewFromCatalog({ _tag: "Ready", games })
  return view._tag === "Shelf" ? view.games : []
}

export function fixtureShelfGame(id: string) {
  const game = fixtureShelfGames([fixtureGame(id)])[0]
  if (game === undefined) throw new Error(`Missing shelf game: ${id}`)
  return game
}

// The name row is visible in the first settings group.
export const savingSettingsModel: SurfaceModel = {
  ...fixtureModel,
  settingsStatus: { _tag: "Saving", settingId: "device-name" },
}
export const failedSettingsModel: SurfaceModel = {
  ...fixtureModel,
  settingsStatus: { _tag: "Problem", settingId: "device-name", message: "The device name could not be saved." },
}
export const emptySettingsModel: SurfaceModel = { ...fixtureModel, settings: [] }
/** The portal's saved-secret row, as clients/portal/src/surface/settings-model.ts
 * publishes it when a key is configured. */
export const secretSettingModel: SurfaceModel = {
  ...fixtureModel,
  settings: [
    ...fixtureModel.settings,
    {
      title: "Metadata",
      items: [{
        id: "steamgriddb-credential",
        label: "SteamGridDB API key",
        value: "Configured",
        description: "Used only by korrid for metadata and cover art lookup",
        interaction: { kind: "sensitiveText", placeholder: "Paste API key", maxLength: 256, clearLabel: "Clear saved key" },
      }],
    },
  ],
}

export function fixtureSettingsConfirmation() {
  const row = picoSettingsViewFromModel(fixtureModel).groups
    .flatMap(group => group.rows).find(row => row.id === "reset")
  if (row?.control.kind !== "action" || row.control.confirmation === undefined) {
    throw new Error("Missing fixture confirmation")
  }
  return { actionId: row.control.actionId, confirmation: row.control.confirmation }
}

// Existing overlay test copy, with the real host retry policy.
export const retryProblem = {
  _tag: "Problem", kicker: "STREAM DROPPED", reason: "zao stopped answering.", canRetry: true,
} satisfies SurfaceStatus

// Deliberately inert display values, never credentials or a usable backup.
export const retiredPublicKey = "preview-retired-public-key"
export const backupText = "PREVIEW ONLY — not an identity backup"
export function identityFixture(status: SurfaceIdentityStatus): SurfaceIdentityManagement {
  return { localBackupAvailable: true, retiredPublicKeys: [retiredPublicKey], status }
}
