import type { SurfaceGameplayOverlayPresentation } from "@contracts/surface/korri-surface"
import { useRef, useSyncExternalStore } from "react"
import {
  overlayChoice,
  overlayDefault,
  overlayDestructive,
  overlayDisabled,
  overlayGroups,
  overlayMany,
  overlayNoTitle,
  overlayRange,
  overlayToggle,
} from "../fixtures/fixture-layers"
import { createOverlayProgram } from "../fixtures/layer-program"
import { ZonePartFrame } from "../fixtures/ZonePartFrame"
import { ZonePaused } from "./ZonePaused"

export const name = "Paused"
export const note = "Mania's pause screen: the red band, the greyed ground, the yellow wedge of Korri's controls"

export const composition = {
  default: [
    { part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "default" },
    { part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "Destructive" },
  ],
  Toggle: [{ part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "Toggle" }],
  Choice: [{ part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "Choice" }],
  Range: [{ part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "Range" }],
  RangeMoving: [{ part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "RangeMoving" }],
  Disabled: [{ part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "Disabled" }],
  Destructive: [
    { part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "Destructive" },
    { part: "src/ui/molecules/ZoneDialog.molecule.part.tsx", state: "FocusedNo" },
  ],
  Groups: [
    { part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "Toggle" },
    { part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "Choice" },
    { part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "Range" },
  ],
  ManyControls: [
    { part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "Disabled" },
    { part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "LongLabel" },
  ],
} as const

/** Unexported: the real page on the real overlay model, a pretend Korri applying each value. */
function Scenario({
  overlay,
  focus,
  moving,
  asking = false,
}: {
  readonly overlay: SurfaceGameplayOverlayPresentation
  /** The control the cursor starts on; the first when absent. */
  readonly focus?: string
  /** A range's value ahead of Korri, for the focused control. */
  readonly moving?: number
  /** Start with the focused (destructive) control asking NO / YES. */
  readonly asking?: boolean
}) {
  const program = useRef<ReturnType<typeof createOverlayProgram> | null>(null)
  program.current ??= createOverlayProgram(overlay, {
    focusedId: focus,
    moving: focus === undefined || moving === undefined ? {} : { [focus]: moving },
    asking: asking && focus !== undefined ? { controlId: focus } : undefined,
  })
  const { dispatch, subscribe, get } = program.current
  const { model, context } = useSyncExternalStore(subscribe, get)
  return (
    <ZonePaused
      answer={model.answer}
      asking={model.asking}
      focusedId={model.focusedId}
      moving={model.moving}
      onAnswer={(yes) => dispatch({ _tag: "ChoseAnswer", yes })}
      onFocusAnswer={(index) => dispatch({ _tag: "FocusedAnswer", index })}
      onFocusControl={(controlId) => dispatch({ _tag: "FocusedControl", controlId })}
      onPressControl={(controlId) => dispatch({ _tag: "PressedControl", controlId })}
      onStepControl={(controlId, direction) => dispatch({ _tag: "SteppedControl", controlId, direction })}
      overlay={context}
    />
  )
}

/** CONTINUE, RESTART, EXIT: the cursor on CONTINUE. */
export default function Default() {
  return <Scenario overlay={overlayDefault} />
}

/** A toggle focused: OFF between the red triangles. */
export function Toggle() {
  return <Scenario focus="fill-screen" overlay={overlayToggle} />
}

/** A choice focused: SHARP between the red triangles. */
export function Choice() {
  return <Scenario focus="screen-filter" overlay={overlayChoice} />
}

/** A range focused: the number and the meter. */
export function Range() {
  return <Scenario focus="volume" overlay={overlayRange} />
}

/** A range moving ahead of Korri: the number in yellow, the meter at the new value. */
export function RangeMoving() {
  return <Scenario focus="volume" moving={85} overlay={overlayRange} />
}

/** A disabled control focused: dimmed, Korri's reason under it. */
export function Disabled() {
  return <Scenario focus="save-state" overlay={overlayDisabled} />
}

/** RESTART asks first: NO lit before YES. */
export function Destructive() {
  return <Scenario asking focus="restart" overlay={overlayDestructive} />
}

/** Korri's controls, then DISPLAY and SOUND under their labels. */
export function Groups() {
  return <Scenario focus="screen-filter" overlay={overlayGroups} />
}

/** Thirteen controls in three groups, the cursor deep in the last: the wedge scrolls to it. */
export function ManyControls() {
  return <Scenario focus="cheats" overlay={overlayMany} />
}

/** Korri gives no title: the band reads PAUSED. */
export function NoTitle() {
  return <Scenario overlay={overlayNoTitle} />
}

/** A narrow column: the wedge takes the width, the rows clear the band. */
export function Narrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Scenario focus="volume" overlay={overlayGroups} />
    </ZonePartFrame>
  )
}
