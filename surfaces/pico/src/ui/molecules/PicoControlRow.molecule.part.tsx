import { fixtureModel, fixtureOverlay } from "../../fixtures/fixture-host"
import { picoOverlayViewFrom } from "../../pico-overlay-view"
import { PicoControlRow } from "./PicoControlRow"

export const name = "Control Row"
export const note = "Korri's label left, state right; disabled is dimmed with its reason, not hidden"

export default function PicoControlRowPart() {
  const control = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status).groups[0]!.controls[3]!
  return <PicoControlRow control={control} onActivate={() => undefined} />
}
