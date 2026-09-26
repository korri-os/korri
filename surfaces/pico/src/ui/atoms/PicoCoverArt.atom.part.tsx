import { fixtureModel } from "../../fixtures/fixture-host"
import { PicoCoverArt } from "./PicoCoverArt"

export const name = "Cover Art"
export const note = "Remapped to the sixteen at its own shape; initials when Korri has no art"

const game = fixtureModel.catalog._tag === "Ready" ? fixtureModel.catalog.games[1] : undefined

export default function PicoCoverArtPart() {
  return <PicoCoverArt artUrl={game?.coverArtUrl} id="hollow" title="Hollow Knight" />
}
