import { fixtureModel } from "../../fixtures/fixture-host"
import { noArtworkGame } from "../../fixtures/named-states"
import { PicoPartFrame } from "../../fixtures/PicoPartFrame"
import { PICO_ART_CAVERN, PICO_ART_SUMMIT } from "../../fixtures/sample-art"
import { PicoCoverArt } from "./PicoCoverArt"

export const name = "Cover Art"
export const note = "Remapped to the sixteen at its own shape; initials when Korri has no art"

const game = fixtureModel.catalog._tag === "Ready" ? fixtureModel.catalog.games[1] : undefined

export default function PicoCoverArtPart() {
  return <PicoCoverArt artUrl={game?.coverArtUrl} id="hollow" title="Hollow Knight" />
}

export function MissingArt() {
  return <PicoPartFrame><PicoCoverArt
    artUrl={noArtworkGame.coverArtUrl} id={noArtworkGame.id} title={noArtworkGame.title}
  /></PicoPartFrame>
}

export function WideArt() {
  return <PicoCoverArt artUrl={PICO_ART_CAVERN} id="spelunky" title="Spelunky" />
}

export function SquareArt() {
  return <PicoCoverArt artUrl={PICO_ART_SUMMIT} id="celeste" title="Celeste Classic" />
}
