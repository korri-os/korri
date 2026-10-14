import { fixtureModel } from "../../fixtures/fixture-host"
import { fixtureShelfGames } from "../../fixtures/named-states"
import { PicoGamesFound } from "./PicoGamesFound"

export const name = "Games Found"
export const note = "What Korri found to play, as carts on a floor line, and a rescan where Korri offers one"

const games = fixtureModel.catalog._tag === "Ready" ? fixtureShelfGames(fixtureModel.catalog.games) : []

export default function PicoGamesFoundPart() {
  return <PicoGamesFound onContinue={() => undefined} onDismissProblem={() => undefined} onScan={() => undefined} view={{ _tag: "Found", games, canScan: true }} />
}

export function Scanning() {
  return <PicoGamesFound onContinue={() => undefined} onDismissProblem={() => undefined} onScan={() => undefined} view={{ _tag: "Scanning" }} />
}

export function Empty() {
  return <PicoGamesFound onContinue={() => undefined} onDismissProblem={() => undefined} onScan={() => undefined} view={{ _tag: "Empty", canScan: true }} />
}

export function Failed() {
  return (
    <PicoGamesFound
      onContinue={() => undefined}
      onDismissProblem={() => undefined}
      onScan={() => undefined}
      view={{ _tag: "Failed", message: "The library could not be read.", canScan: false, problem: "The game folders could not be scanned." }}
    />
  )
}
