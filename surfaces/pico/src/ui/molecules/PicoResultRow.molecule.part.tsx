import { fixtureModel } from "../../fixtures/fixture-host"
import { picoLibraryViewFrom, PICO_ALL_SECTIONS } from "../../pico-library-view"
import { PicoResultRow } from "./PicoResultRow"

export const name = "Result Row"
export const note = "A list is read down the left edge; the cart is the shelf's idiom"

export default function PicoResultRowPart() {
  const game = picoLibraryViewFrom(fixtureModel.catalog, "", PICO_ALL_SECTIONS).results[1]!
  return <PicoResultRow game={game} onOpen={() => undefined} />
}
