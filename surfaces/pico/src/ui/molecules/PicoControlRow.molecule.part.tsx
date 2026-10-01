import { fixtureModel, fixtureOverlay } from "../../fixtures/fixture-host"
import { picoOverlayViewFrom } from "../../pico-overlay-view"
import { PicoControlRow } from "./PicoControlRow"

export const name = "Control Row"
export const note = "Korri's label left, state right; disabled is dimmed with its reason, not hidden"

export default function PicoControlRowPart() {
  const control = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status).groups[0]!.controls[3]!
  return <PicoControlRow control={control} onActivate={() => undefined} onAdjust={() => undefined} />
}

export function DisabledWithReason() {
  const control = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status).groups[0]!.controls[1]!
  return <PicoControlRow control={control} onActivate={() => undefined} onAdjust={() => undefined} />
}

export function DestructiveCommand() {
  const control = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status).controls[1]!
  return <PicoControlRow control={control} onActivate={() => undefined} onAdjust={() => undefined} />
}

export function RangeValue() {
  const control = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status).groups[0]!.controls[4]!
  return <PicoControlRow control={control} onActivate={() => undefined} onAdjust={() => undefined} />
}
