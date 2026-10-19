/**
 * The twelve games as card facts, with the mockup's art assignment.
 *
 * A card is drawn from a SurfaceGame, so every field here is one Korri
 * publishes: the title, the subtitle as the system, the launch locations as
 * places, and `resumable`. The art is the sample art by cover shape; the
 * five games the mockup left without art stay without, so the stripe wells
 * are always in view.
 */
import type { SurfaceGame } from "@contracts/surface/korri-surface"
import type { ZoneCardFacts, ZonePlace } from "../ui/molecules/ZoneCard"
import { fixtureGames } from "./fixture-host"
import {
  ZONE_ART_BLOCKS,
  ZONE_ART_BRAMBLE,
  ZONE_ART_CAVERN,
  ZONE_ART_KNIGHT,
  ZONE_ART_SHORE,
  ZONE_ART_SUMMIT,
  ZONE_ART_TOWER,
} from "./sample-art"

export interface ZoneCardFixture extends ZoneCardFacts {
  readonly id: string
}

const ART: Readonly<Record<string, string>> = {
  celeste: ZONE_ART_SUMMIT,
  "hollow-knight": ZONE_ART_KNIGHT,
  tetris: ZONE_ART_BLOCKS,
  spelunky: ZONE_ART_CAVERN,
  "lantern-keep": ZONE_ART_TOWER,
  "tide-pool": ZONE_ART_SHORE,
  "bramble-run": ZONE_ART_BRAMBLE,
}

/** The fixture host names the local device "here"; a game with no choice plays here. */
const placesOf = (game: SurfaceGame): readonly ZonePlace[] =>
  (game.launchLocations ?? [{ id: "here", label: "This device" }])
    .map(location => ({ label: location.label, here: location.id === "here" }))

export const cardOf = (game: SurfaceGame): ZoneCardFixture => ({
  id: game.id,
  title: game.title,
  system: game.subtitle,
  places: placesOf(game),
  resumable: game.resumable === true,
  art: ART[game.id],
})

export const fixtureCards: readonly ZoneCardFixture[] = fixtureGames.map(cardOf)

export const fixtureCard = (id: string): ZoneCardFixture => {
  const found = fixtureCards.find(card => card.id === id)
  if (found === undefined) throw new Error(`no fixture card ${id}`)
  return found
}

/** Forty cards: the twelve, repeated with numbered titles, for a long band. */
export const fixtureCardsMany: readonly ZoneCardFixture[] = Array.from({ length: 40 }, (_, index) => {
  const base = fixtureCards[index % fixtureCards.length]!
  return index < fixtureCards.length ? base : { ...base, id: `${base.id}-${index}`, title: `${base.title} ${index + 1}` }
})
