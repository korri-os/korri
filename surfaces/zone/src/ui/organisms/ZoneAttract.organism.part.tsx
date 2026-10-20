import { fixtureGames } from "../../fixtures/fixture-host"
import { ZONE_ART_SHORE } from "../../fixtures/sample-art"
import { useFixtureHold } from "../../fixtures/fixture-moments"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneAttract } from "./ZoneAttract"

export const name = "Attract"
export const note = "Idle: each game's title card, then its stage panning with the HUD and PRESS ANY BUTTON, 6 s a game (M6); window.seek(t) holds a frame"

const tide = fixtureGames[5]

/** Tide Pool's turn, 1.5 s in: its title card at rest. */
export default function Turn() {
  useFixtureHold(1500)
  return (
    <ZoneAttract act={8} art={ZONE_ART_SHORE} clock="21:47" plays={7} system={tide?.subtitle} title={tide?.title ?? ""} turn="tide" />
  )
}

/** Tide Pool's stage, 4 s into its turn: the art panning, the HUD, PRESS ANY BUTTON. */
export function Stage() {
  useFixtureHold(4000)
  return (
    <ZoneAttract act={8} art={ZONE_ART_SHORE} clock="21:47" plays={7} system={tide?.subtitle} title={tide?.title ?? ""} turn="tide" />
  )
}

/** No art: the stripe pattern stands as the stage; never played, PLAYS --. */
export function NoArt() {
  useFixtureHold(4000)
  return (
    <ZoneAttract act={1} clock="21:47" system="PC" title="Comet Courier" turn="comet" />
  )
}

/** A narrow-tall screen. */
export function NarrowTall() {
  useFixtureHold(1500)
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneAttract act={8} art={ZONE_ART_SHORE} clock="21:47" plays={7} system={tide?.subtitle} title={tide?.title ?? ""} turn="tide" />
    </ZonePartFrame>
  )
}
