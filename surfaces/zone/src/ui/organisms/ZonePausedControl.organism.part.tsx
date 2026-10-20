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
import { useFixtureDirections } from "../../fixtures/fixture-directions"
import { createOverlayProgram } from "../../fixtures/layer-program"
import { ZonePausedControl } from "./ZonePausedControl"

export const name = "Paused control"
export const note = "One row of PAUSED's wedge: a black parallelogram, the label, and what Korri's interaction shows"

/* The helpers Caliper hands an authored check, typed locally: Zone imports
 * nothing from Caliper (test/authoring-gate.test.ts). */
interface CheckTools {
  readonly canvas: { getByRole(role: string, options: { name: RegExp }): HTMLElement }
  readonly input: { click(element: Element): Promise<void> }
  readonly expect: (value: unknown) => { toBe(expected: unknown): void }
  readonly waitFor: (assertion: () => void) => Promise<void>
}

/* Left and right reach the control the way the portal delivers them
 * (clients/portal/src/input/spatial-focus.ts), never as keys. Volume is 0
 * to 100 in steps of 5, at 60. */
export const checks = {
  Range: {
    "the portal's right raises Volume by Korri's step and left lowers it": async ({ canvas, input, expect, waitFor }: CheckTools) => {
      const volume = () => canvas.getByRole("button", { name: /Volume/ })
      const value = () => volume().querySelector(".zone-paused-control-text")?.textContent
      await input.click(volume())
      const slide = (direction: "left" | "right") =>
        volume().dispatchEvent(new CustomEvent("korri-semantic-direction", {
          detail: { direction, repeat: false, releaseExpected: false },
        }))
      slide("right")
      await waitFor(() => expect(value()).toBe("65"))
      slide("left")
      slide("left")
      await waitFor(() => expect(value()).toBe("55"))
    },
  },
}

/** Unexported: one control on the real overlay model, a pretend Korri applying each value. */
function Scenario({ control, moving }: { readonly control: SurfaceGameplayControl; readonly moving?: number }) {
  const program = useRef<ReturnType<typeof createOverlayProgram> | null>(null)
  program.current ??= createOverlayProgram(
    { kind: "gameplay-overlay", controls: [control], groups: [] },
    moving === undefined ? {} : { moving: { [control.id]: moving } },
  )
  const { dispatch, subscribe, get } = program.current
  const { model, context } = useSyncExternalStore(subscribe, get)
  useFixtureDirections()
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
