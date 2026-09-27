import { fixtureGame, longTitleGame, fixtureShelfGames } from "../../fixtures/named-states"
import { fixtureModel } from "../../fixtures/fixture-host"
import { picoHomeViewFromCatalog } from "../../pico-home-view"
import { PicoResumeList } from "./PicoResumeList"

// Intentionally null: no resume heading when there is nothing to resume.
export function NoResumableGames() {
  return <PicoResumeList games={[]} onOpen={() => undefined} />
}

export function OneGame() {
  return <PicoResumeList games={fixtureShelfGames([fixtureGame("hollow")])} onOpen={() => undefined} />
}

export function LongTitleNoArtwork() {
  const game = { ...longTitleGame, resumable: true, coverArtUrl: undefined }
  return <PicoResumeList games={fixtureShelfGames([game])} onOpen={() => undefined} />
}

export const name = "Resume List"
export const note = "Absent when nothing resumes; an empty Resume reads as lost saves"

export default function PicoResumeListPart() {
  const view = picoHomeViewFromCatalog(fixtureModel.catalog)
  const games = view._tag === "Shelf" ? view.games.filter((game) => game.resumable === true) : []
  return <PicoResumeList games={games} onOpen={() => undefined} />
}
