import { PICO_SAMPLE_WIDE_ART } from "../../fixtures/sample-art"
import { PicoKeyArt } from "./PicoKeyArt"

export const name = "Key Art"
export const note = "The focused game's wide art, remapped and dimmed behind the shelf"

export default function PicoKeyArtPart() {
  return <PicoKeyArt src={PICO_SAMPLE_WIDE_ART} />
}
