import { useState } from "react"
import { fixtureModel, fixtureOverlay } from "../../fixtures/fixture-host"
import { picoOverlayViewFrom, picoRangeStep } from "../../pico-overlay-view"
import { PicoControlRow } from "./PicoControlRow"

export const name = "Control Row"
export const note = "Korri's label left, state right; disabled is dimmed with its reason, not hidden"

export default function PicoControlRowPart() {
  const control = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status).groups[0]!.controls[3]!
  return <PicoControlRow control={control} onActivate={() => undefined} />
}

export function DisabledWithReason() {
  const control = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status).groups[0]!.controls[1]!
  return <PicoControlRow control={control} onActivate={() => undefined} />
}

export function DestructiveCommand() {
  const control = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status).controls[1]!
  return <PicoControlRow control={control} onActivate={() => undefined} />
}

export function RangeValue() {
  const control = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status).groups[0]!.controls[4]!
  return <PicoControlRow control={control} onActivate={() => undefined} />
}

// The surface keeps a range's value until Korri hears it; here the part does,
// so left and right move the slider.
export function RangeSelectable() {
  const control = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status).groups[0]!.controls[4]!
  const [value, setValue] = useState(control.range?.value)
  return <PicoControlRow control={control} onActivate={() => undefined} value={value}
    onStep={request => setValue(current => control.range === undefined || current === undefined
      ? current : picoRangeStep({ ...control.range, value: current }, request.way))} />
}
