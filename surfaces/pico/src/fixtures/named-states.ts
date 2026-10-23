import type {
  SurfaceAction,
  SurfaceGameplayOverlayPresentation,
  SurfaceInvite,
  SurfaceGame,
  SurfaceIdentityManagement,
  SurfaceIdentityStatus,
  SurfaceModel,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"
import { picoHomeViewFromCatalog } from "../pico-home-view"
import { picoInvitesFrom } from "../pico-invite-view"
import { picoSettingsViewFromModel } from "../pico-settings-view"
import { fixtureModel, fixtureOverlay } from "./fixture-host"
import { PICO_ART_BRAMBLE, PICO_ART_COMET, PICO_ART_SHORE } from "./sample-art"

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
    .flatMap(group => group.rows).find(row => row.id === "game-folder:loc-a")
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

/* ---- Stream play ---- */

/** The actions Korri publishes with an invite: the route this device can
 * take, then a dismiss. The ids are this fixture's own; Pico hands them back
 * unchanged. */
export function fixtureInviteActions(inviteId: string): readonly SurfaceAction[] {
  return [
    { id: `${inviteId}:stream`, label: "Play on stream", enabled: true },
    { id: `${inviteId}:dismiss`, label: "Dismiss", enabled: true },
  ]
}

/** Simon is playing Turtles in Time on the Odin 2 Portal: the outcome the
 * feature exists for. */
export const fixtureInvite: SurfaceInvite = {
  id: "invite-odin-turtles",
  personName: "Simon",
  gameTitle: "Turtles in Time",
  deviceLabel: "Odin 2 Portal",
  coverArtUrl: PICO_ART_SHORE,
  actions: fixtureInviteActions("invite-odin-turtles"),
}

/** A second person, on a device with a person name of its own. */
export const fixtureSecondInvite: SurfaceInvite = {
  id: "invite-tv-comet",
  personName: "Ana",
  gameTitle: "Comet Courier",
  deviceLabel: "Living room",
  coverArtUrl: PICO_ART_COMET,
  actions: fixtureInviteActions("invite-tv-comet"),
}

/** The same person on a third device. */
export const fixtureThirdInvite: SurfaceInvite = {
  id: "invite-desk-bramble",
  personName: "Simon",
  gameTitle: "Bramble Run",
  deviceLabel: "Desk PC",
  coverArtUrl: PICO_ART_BRAMBLE,
  actions: fixtureInviteActions("invite-desk-bramble"),
}

/** The longest values the treaty allows a name to reach: a 64-character
 * person name with no break in it, a long title, a long device label, and
 * no cover art. */
export const longInvite: SurfaceInvite = {
  id: "invite-mini-zelda",
  personName: "Maximilian-Alexander-Bartholomew-Featherstonehaugh-Wolfeschlegel",
  gameTitle: "The Legend of Zelda: A Link to the Past & Four Swords",
  deviceLabel: "Retroid Pocket Mini V2 in the upstairs office",
  actions: fixtureInviteActions("invite-mini-zelda"),
}

/** Stream play is offered but cannot start now: Korri dims the route. */
export const dimmedInvite: SurfaceInvite = {
  ...fixtureInvite,
  actions: [
    { id: "invite-odin-turtles:stream", label: "Play on stream", description: "Odin 2 Portal is busy", enabled: false },
    { id: "invite-odin-turtles:dismiss", label: "Dismiss", enabled: true },
  ],
}

export const fixtureInvites: readonly SurfaceInvite[] = [fixtureInvite, fixtureSecondInvite, fixtureThirdInvite]

/** More invites than PLAYING NOW or the notice stack can show at once. */
export const manyInvites: readonly SurfaceInvite[] = [
  ...fixtureInvites,
  longInvite,
  ...["Rgds", "Steam Deck"].map((deviceLabel, index): SurfaceInvite => ({
    ...fixtureSecondInvite,
    id: `invite-more-${index}`,
    deviceLabel,
    actions: fixtureInviteActions(`invite-more-${index}`),
  })),
]

/** Invites as Pico draws them, for the parts below the binding. */
export const inviteViews = (invites: readonly SurfaceInvite[]) => picoInvitesFrom({ invites })

/** Invites as notices say them, newest first. */
export const inviteNoticeViews = (invites: readonly SurfaceInvite[]) =>
  [...invites].reverse().map(invite => ({
    id: invite.id,
    personName: invite.personName,
    gameTitle: invite.gameTitle,
    deviceLabel: invite.deviceLabel,
  }))

export const inviteModel = (invites: readonly SurfaceInvite[]): SurfaceModel => ({ ...fixtureModel, invites })

/** Player 1's overlay while two other devices play on the stream. */
export const streamPlayersOverlay: SurfaceGameplayOverlayPresentation = {
  ...fixtureOverlay,
  streamPlayers: ["Ana", "Living room"],
}

/** The overlay with the longest names: a person name with no break, and a
 * device label standing in for a person with none. */
export const longStreamPlayersOverlay: SurfaceGameplayOverlayPresentation = {
  ...fixtureOverlay,
  streamPlayers: [longInvite.personName, "Ana", "Retroid Pocket Mini V2 in the upstairs office"],
}

/** The host's words while this device connects to another's stream. */
export const streamConnectingStatus = {
  _tag: "Busy",
  kicker: "Connecting to Odin 2 Portal…",
  detail: "Turtles in Time",
  actions: [{ id: "stream:cancel", label: "Cancel", enabled: true }],
} satisfies SurfaceStatus

export const streamFailedStatus = {
  _tag: "Problem",
  kicker: "Couldn't play on the stream",
  reason: "Odin 2 Portal did not answer. Check that it is on and on this network.",
  canRetry: true,
  gameTitle: "Turtles in Time",
} satisfies SurfaceStatus

/** Player 1 left the game on the device that holds it. */
export const streamPausedStatus = {
  _tag: "Paused",
  kicker: "Paused on Odin 2 Portal",
  detail: "Turtles in Time goes on when Simon returns to it.",
  actions: [{ id: "stream:leave", label: "Leave", enabled: true }],
} satisfies SurfaceStatus

export const streamEndedStatus = {
  _tag: "Ended",
  kicker: "Session ended",
  reason: "Turtles in Time ended on Odin 2 Portal.",
  gameTitle: "Turtles in Time",
} satisfies SurfaceStatus
