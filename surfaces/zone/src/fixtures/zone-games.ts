/**
 * The GAMES page's fixtures: the mockup's twelve games with their art, the
 * catalog in each of Korri's four states, a sectioned library, forty games,
 * one game, and Korri's actions for GAME OPTS.
 *
 * Every value is typed against the treaty, so a field Korri does not
 * publish is a type error here. Times are fixed instants, never the clock.
 */
import type {
  SurfaceAction,
  SurfaceCatalog,
  SurfaceGame,
  SurfaceHost,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureGames, fixtureModel, type FixtureHost } from "./fixture-host"
import {
  ZONE_ART_BLOCKS,
  ZONE_ART_BRAMBLE,
  ZONE_ART_CAVERN,
  ZONE_ART_COMET,
  ZONE_ART_KNIGHT,
  ZONE_ART_SHORE,
  ZONE_ART_SUMMIT,
  ZONE_ART_TOWER,
} from "./sample-art"

const ART: Readonly<Record<string, string>> = {
  celeste: ZONE_ART_SUMMIT,
  "hollow-knight": ZONE_ART_KNIGHT,
  tetris: ZONE_ART_BLOCKS,
  spelunky: ZONE_ART_CAVERN,
  "lantern-keep": ZONE_ART_TOWER,
  "tide-pool": ZONE_ART_SHORE,
  "comet-courier": ZONE_ART_COMET,
  "bramble-run": ZONE_ART_BRAMBLE,
}

/** 19 Oct 2026, 12:00 UTC: the same day in every time zone from -11 to +11. */
export const FIXTURE_LAST_PLAYED = 1_792_411_200_000

/** Session facts as Korri records them: all three together, or none. */
const FACTS: Readonly<Record<string, Pick<SurfaceGame, "lastPlayedAt" | "playCount" | "totalPlaytimeSeconds">>> = {
  celeste: { lastPlayedAt: FIXTURE_LAST_PLAYED, playCount: 23, totalPlaytimeSeconds: 41_460 },
  "hollow-knight": { lastPlayedAt: FIXTURE_LAST_PLAYED - 86_400_000 * 9, playCount: 7, totalPlaytimeSeconds: 11_520 },
  spelunky: { lastPlayedAt: FIXTURE_LAST_PLAYED - 86_400_000 * 2, playCount: 1, totalPlaytimeSeconds: 2_460 },
}

/** The twelve, with art where the mockup had it and facts where Korri recorded play. */
export const fixtureLibrary: readonly SurfaceGame[] = fixtureGames.map(game => ({
  ...game,
  ...(ART[game.id] === undefined ? {} : { coverArtUrl: ART[game.id] }),
  ...FACTS[game.id],
}))

export const gamesModel = (catalog: SurfaceCatalog): SurfaceModel => ({ ...fixtureModel, catalog })

export const fixtureReadyModel = gamesModel({ _tag: "Ready", games: fixtureLibrary })
export const fixtureLoadingModel = gamesModel({ _tag: "Loading" })
export const fixtureErrorModel = gamesModel({
  _tag: "Error",
  message: "The library could not be read. The disk did not answer.",
})
export const fixtureEmptyModel = gamesModel({ _tag: "Empty" })

const byId = (id: string): SurfaceGame => {
  const found = fixtureLibrary.find(game => game.id === id)
  if (found === undefined) throw new Error(`no fixture game ${id}`)
  return found
}

/** Korri's grouping: what to continue, what is on this device, what zao has. */
export const fixtureSectionedGames: readonly SurfaceGame[] = [
  { ...byId("celeste"), section: "Continue" },
  ...["hollow-knight", "spelunky", "lantern-keep", "comet-courier", "bramble-run", "wario-land-4"]
    .map(id => ({ ...byId(id), section: "This device" })),
  ...["tetris", "tide-pool", "oracle-of-seasons"].map(id => ({ ...byId(id), section: "zao" })),
  ...["petal-quest", "super-metroid"].map(id => ({ ...byId(id), section: "Supercalifragilisticexpialidocious-handheld" })),
]
export const fixtureSectionsModel = gamesModel({ _tag: "Ready", games: fixtureSectionedGames })

export const fixtureOneGameModel = gamesModel({ _tag: "Ready", games: [byId("hollow-knight")] })

/** Wario Land 4 with wide art (2:1) and no cover: the card letterboxes the wide art in its well. */
export const fixtureWideArtGames: readonly SurfaceGame[] = fixtureLibrary.map(game =>
  game.id === "wario-land-4" ? { ...game, wideArtUrl: ZONE_ART_BRAMBLE } : game)
export const fixtureWideArtModel = gamesModel({ _tag: "Ready", games: fixtureWideArtGames })
export const fixtureWideArtIndex = fixtureWideArtGames.findIndex(game => game.id === "wario-land-4")

/** Forty games: the twelve, then numbered copies, for a band longer than any screen. */
export const fixtureFortyGames: readonly SurfaceGame[] = Array.from({ length: 40 }, (_, index) => {
  const base = fixtureLibrary[index % fixtureLibrary.length]!
  return index < fixtureLibrary.length ? base : { ...base, id: `${base.id}-${index}`, title: `${base.title} ${index + 1}` }
})
export const fixtureFortyModel = gamesModel({ _tag: "Ready", games: fixtureFortyGames })

/** Korri's actions for a game: one plain, one destructive, one it cannot do now. */
export const fixtureGameActions: readonly SurfaceAction[] = [
  { id: "verify-files", label: "Verify files", description: "Check the game's files against the library", enabled: true },
  {
    id: "delete-save",
    label: "Delete save",
    description: "The save on this device is removed. Saves on zao stay.",
    enabled: true,
    destructive: true,
  },
  { id: "copy-to-zao", label: "Copy to zao", description: "zao is not on the network", enabled: false },
]

/** The fixture host, answering gameActions with the actions above. */
export function createGamesFixtureHost(actions: readonly SurfaceAction[] = fixtureGameActions): FixtureHost & SurfaceHost {
  const base = createFixtureHost()
  return Object.assign(base, { gameActions: () => actions })
}
