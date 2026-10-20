import type { SurfaceGameplayControl } from "@contracts/surface/korri-surface"
import { useRef, useSyncExternalStore } from "react"
import {
  controlFill,
  controlFilter,
  controlRestart,
  controlResume,
  controlSave,
  controlVolume,
} from "../../fixtures/fixture-layers"
import { createOverlayProgram } from "../../fixtures/layer-program"
import { ZonePausedControl } from "./ZonePausedControl"

export const name = "Paused control"
export const note = "One row of PAUSED's wedge: a black parallelogram, the label, and what Korri's interaction shows"

/** Unexported: one control on the real overlay model, a pretend Korri applying each value. */
function Scenario({ control, moving }: { readonly control: SurfaceGameplayControl; readonly moving?: number }) {
  const program = useRef<ReturnType<typeof createOverlayProgram> | null>(null)
  program.current ??= createOverlayProgram(
    { kind: "gameplay-overlay", controls: [control], groups: [] },
    moving === undefined ? {} : { moving: { [control.id]: moving } },
  )
  const { dispatch, subscribe, get } = program.current
  const { model, context } = useSyncExternalStore(subscribe, get)
  const shown = context.controls[0] ?? control
  return (
    <ZonePausedControl
      control={shown}
      focused={model.focusedId === shown.id}
      moving={model.moving[shown.id] !== undefined}
      onFocus={() => dispatch({ _tag: "FocusedControl", controlId: shown.id })}
      onPress={() => dispatch({ _tag: "PressedControl", controlId: shown.id })}
      onStep={(direction) => dispatch({ _tag: "SteppedControl", controlId: shown.id, direction })}
      rangeValue={model.moving[shown.id]}
    />
  )
}

/** A command, focused: the pink underline under the parallelogram. */
export default function Command() {
  return <Scenario control={controlResume} />
}

/** A toggle: Korri's false label; left, right and A flip it. */
export function Toggle() {
  return <Scenario control={controlFill} />
}

/** A choice: the chosen option between red triangles. */
export function Choice() {
  return <Scenario control={controlFilter} />
}

/** A range: the number and the meter. */
export function Range() {
  return <Scenario control={controlVolume} />
}

/** A range on the move: the number in yellow, ahead of Korri. */
export function RangeMoving() {
  return <Scenario control={controlVolume} moving={85} />
}

/** At the top of its range: the right triangle dims. */
export function RangeAtMax() {
  return <Scenario control={{ ...controlVolume, interaction: { kind: "range", value: 100, min: 0, max: 100, step: 5 } }} />
}

/** Disabled: dimmed, Korri's reason under it, still reachable. */
export function Disabled() {
  return <Scenario control={controlSave} />
}

/** Destructive: the label in red. */
export function Destructive() {
  return <Scenario control={controlRestart} />
}

/** A long label with one unbroken word: the label wraps, the value keeps its room. */
export function LongLabel() {
  return (
    <Scenario
      control={{
        ...controlFilter,
        label: "Screen filter for the second linked handheld supercalifragilisticexpialidocious",
      }}
    />
  )
}
