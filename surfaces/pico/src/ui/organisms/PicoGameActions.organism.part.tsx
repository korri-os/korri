import { createFixtureHost } from "../../fixtures/fixture-host"
import { PicoGameActions } from "./PicoGameActions"

// Intentionally null: games without actions must not show an ACTIONS heading.
export function NoActions() {
  return <PicoGameActions actions={createFixtureHost().gameActions("petal")} onRun={() => undefined} />
}

export function DisabledAction() {
  return <PicoGameActions actions={createFixtureHost().gameActions("hollow").filter(action => !action.enabled)} onRun={() => undefined} />
}

export const name = "Game Actions"
export const note = "Absent when Korri offers none; an empty ACTIONS teaches people to stop reading"

export default function PicoGameActionsPart() {
  return (
    <PicoGameActions actions={createFixtureHost().gameActions("hollow")} onRun={() => undefined} />
  )
}
