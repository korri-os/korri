import { PICO_SAMPLE_WIDE_ART } from "../../fixtures/sample-art"
import { PicoKeyArtStage } from "./PicoKeyArtStage"

export const name = "Key Art Stage"
export const note = "Art and its scrim as one thing; the shelf and a game's screen both stand on it"

export default function PicoKeyArtStagePart() {
  return <PicoKeyArtStage src={PICO_SAMPLE_WIDE_ART} />
}
