import { fixtureGames } from "../../fixtures/fixture-host"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneLaunchCard } from "./ZoneLaunchCard"

export const name = "Launch card"
export const note = "PLAY or CONTINUE chosen: the title card sweeps over the zone select (M3) until Korri lets go of the game"

const celeste = fixtureGames[0]
const zelda = fixtureGames[9]

/** Celeste Classic, played 23 times: ACT 24, PICO-8 on the tag. */
export default function Launching() {
  return (
    <ZoneLaunchCard act={(celeste?.playCount ?? 0) + 1} system={celeste?.subtitle} title={celeste?.title ?? ""} />
  )
}

/** A game never played: ACT 1. */
export function FirstSession() {
  return (
    <ZoneLaunchCard act={1} system="SNES" title="Super Metroid" />
  )
}

/** The longest title: caps-1, wrapped between words. */
export function LongTitle() {
  return (
    <ZoneLaunchCard act={3} system={zelda?.subtitle} title={zelda?.title ?? ""} />
  )
}

/** Korri let go of the game: the card leaves toward the bottom left. */
export function Leaving() {
  return (
    <ZoneLaunchCard act={24} leaving system={celeste?.subtitle} title={celeste?.title ?? ""} />
  )
}

/** A narrow-tall screen: the disc under the words. */
export function NarrowTall() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneLaunchCard act={24} system={celeste?.subtitle} title={celeste?.title ?? ""} />
    </ZonePartFrame>
  )
}
