import { fixtureModel } from "../../fixtures/fixture-host"
import { picoHomeViewFromCatalog } from "../../pico-home-view"
import { PicoResumeList } from "./PicoResumeList"

export const name = "Resume List"
export const note = "Absent when nothing resumes; an empty Resume reads as lost saves"

export default function PicoResumeListPart() {
  const view = picoHomeViewFromCatalog(fixtureModel.catalog)
  const games = view._tag === "Shelf" ? view.games.filter((game) => game.resumable === true) : []
  return <PicoResumeList games={games} onOpen={() => undefined} />
}
