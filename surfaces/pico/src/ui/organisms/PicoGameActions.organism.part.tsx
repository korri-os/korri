import { createFixtureHost } from "../../fixtures/fixture-host"
import { PicoGameActions } from "./PicoGameActions"

export const name = "Game Actions"
export const note = "Absent when Korri offers none; an empty ACTIONS teaches people to stop reading"

export default function PicoGameActionsPart() {
  return (
    <PicoGameActions actions={createFixtureHost().gameActions("hollow")} onRun={() => undefined} />
  )
}
