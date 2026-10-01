import { fixtureModel, fixtureOverlay } from "../../fixtures/fixture-host"
import { type PicoOverlayRange, picoOverlayViewFrom, picoRangeLabel } from "../../pico-overlay-view"
import { PicoSlider } from "./PicoSlider"

export const name = "Slider"
export const note = "One stop: left lowers and right raises by Korri's step; the arrows dim at min and max"

/** The fixture overlay's Volume range, at the value a state needs. */
function volume(at?: (range: PicoOverlayRange) => number): PicoOverlayRange {
  const range = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status)
    .groups.flatMap((group) => group.controls)
    .find((control) => control.range !== undefined)!.range!
  return at === undefined ? range : { ...range, value: at(range) }
}

export default function PicoSliderPart() {
  const range = volume()
  return (
    <PicoSlider
      label="Volume"
      max={range.max}
      min={range.min}
      onLeave={() => undefined}
      onRelease={() => undefined}
      onStep={() => undefined}
      step={range.step}
      value={range.value}
      valueText={picoRangeLabel(range.value, range)}
    />
  )
}

export function AtMin() {
  const range = volume((r) => r.min)
  return (
    <PicoSlider
      label="Volume"
      max={range.max}
      min={range.min}
      onLeave={() => undefined}
      onRelease={() => undefined}
      onStep={() => undefined}
      step={range.step}
      value={range.value}
      valueText={picoRangeLabel(range.value, range)}
    />
  )
}

export function AtMax() {
  const range = volume((r) => r.max)
  return (
    <PicoSlider
      label="Volume"
      max={range.max}
      min={range.min}
      onLeave={() => undefined}
      onRelease={() => undefined}
      onStep={() => undefined}
      step={range.step}
      value={range.value}
      valueText={picoRangeLabel(range.value, range)}
    />
  )
}

export function Disabled() {
  const range = volume()
  return (
    <PicoSlider
      disabled
      label="Volume"
      max={range.max}
      min={range.min}
      onLeave={() => undefined}
      onRelease={() => undefined}
      onStep={() => undefined}
      step={range.step}
      value={range.value}
      valueText={picoRangeLabel(range.value, range)}
    />
  )
}
