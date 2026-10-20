import { fixtureGames } from "../../fixtures/fixture-host"
import { ZONE_ART_SHORE } from "../../fixtures/sample-art"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneAttract } from "./ZoneAttract"

export const name = "Attract"
export const note = "Idle: each game's title card, then its stage panning with the HUD and PRESS ANY BUTTON, 6 s a game (M6); window.seek(t) holds a frame"

const hollow = fixtureGames[1]

/** Hollow Knight's turn: its card, then its art as the stage. */
export default function Turn() {
  return (
    <ZoneAttract act={8} art={ZONE_ART_SHORE} clock="21:47" plays={7} system={hollow?.subtitle} title={hollow?.title ?? ""} turn="hollow" />
  )
}

/** No art: the stripe pattern stands as the stage; never played, PLAYS --. */
export function NoArt() {
  return (
    <ZoneAttract act={1} clock="21:47" system="PC" title="Comet Courier" turn="comet" />
  )
}

/** A narrow-tall screen. */
export function NarrowTall() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneAttract act={8} art={ZONE_ART_SHORE} clock="21:47" plays={7} system={hollow?.subtitle} title={hollow?.title ?? ""} turn="hollow" />
    </ZonePartFrame>
  )
}
