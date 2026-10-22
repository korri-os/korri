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
import { useFixtureDirections } from "../fixtures/fixture-directions"
import { ZONE_ART_BRAMBLE, ZONE_ART_KNIGHT } from "../fixtures/sample-art"
import { createOverlayProgram } from "../fixtures/layer-program"
import { ZonePartFrame } from "../fixtures/ZonePartFrame"
import { ZonePaused } from "./ZonePaused"

export const name = "Paused"
export const note = "Mania's pause screen: the red band, the running game greyed (or a grey checker), the yellow wedge of Korri's controls stepping down-left"

export const composition = {
  OnTheGame: [{ part: "src/ui/organisms/ZonePausedControl.organism.part.tsx", state: "default" }],
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

/* The helpers Caliper hands an authored check, typed locally: Zone imports
 * nothing from Caliper (test/authoring-gate.test.ts). */
interface CheckTools {
  readonly canvas: { getByRole(role: string, options: { name: RegExp }): HTMLElement }
  readonly expect: (value: unknown) => { toBe(expected: unknown): void }
  readonly waitFor: (assertion: () => void) => Promise<void>
}

/**
 * What the wedge shows, read off the real page in Chromium: how far it is
 * scrolled, the rows (controls and group tags) not whole inside its view,
 * whether the focused row and its pink drop stand whole, and how each edge
 * meets the rows. An edge is "clear" when nothing is hidden past it,
 * "letters" when it cuts a line of words within a sixth of the line's box
 * from its middle (a line's box is about 1.6 of its cap, so a fifth of the
 * cap or more shows), else "bad": a sliver with no letters, or a cut that
 * falls in a gap and says nothing about what is past it.
 */
function wedgeView(button: HTMLElement) {
  const list = button.closest<HTMLElement>(".zone-paused-list")
  if (list === null) throw new Error("no wedge list")
  const view = list.getBoundingClientRect()
  const rows = Array.from(list.querySelectorAll<HTMLElement>(".zone-paused-control, .zone-paused-group-label"))
  const hidden = rows.filter(row => {
    const box = row.getBoundingClientRect()
    return box.top < view.top - 0.5 || box.bottom > view.bottom + 0.5
  })
  const lines: DOMRect[] = []
  const walk = document.createTreeWalker(list, NodeFilter.SHOW_TEXT)
  for (let node = walk.nextNode(); node !== null; node = walk.nextNode()) {
    if ((node.textContent ?? "").trim() === "") continue
    const range = document.createRange()
    range.selectNodeContents(node)
    lines.push(...Array.from(range.getClientRects()))
  }
  const edge = (at: number, past: (box: DOMRect) => boolean) => {
    if (!rows.some(row => past(row.getBoundingClientRect()))) return "clear"
    return lines.some(line => Math.abs(at - (line.top + line.bottom) / 2) <= line.height / 6) ? "letters" : "bad"
  }
  const focused = list.querySelector<HTMLElement>(".zone-paused-control[data-focused]")
  const drop = 3 * Number.parseFloat(getComputedStyle(list).getPropertyValue("--zone-px") || "1")
  const whole = focused === null ? false : (() => {
    const box = focused.getBoundingClientRect()
    return box.top >= view.top - 0.5 && box.bottom + drop <= view.bottom + 0.5
  })()
  return {
    scrolled: list.scrollTop,
    hidden: hidden.map(row => row.textContent ?? "").join(", "),
    focusedWhole: whole,
    top: edge(view.top, box => box.top < view.top - 0.5),
    bottom: edge(view.bottom, box => box.bottom > view.bottom + 0.5),
  }
}

/* The wedge scrolls only when its rows do not fit; when it scrolls, the
 * focused row stands whole and each edge either hides nothing or cuts a
 * line of words, never a letterless sliver or a gap. */
export const checks = {
  Groups: {
    "seven rows fit the wedge: none scrolled away, none cut": async ({ canvas, expect, waitFor }: CheckTools) => {
      await waitFor(() => {
        const seen = wedgeView(canvas.getByRole("button", { name: /Screen filter/ }))
        expect(seen.hidden).toBe("")
        expect(seen.scrolled).toBe(0)
        expect(seen.focusedWhole).toBe(true)
      })
    },
  },
  ManyControls: {
    "the deep focus stands whole and the cut edge shows letters": async ({ canvas, expect, waitFor }: CheckTools) => {
      await waitFor(() => {
        const seen = wedgeView(canvas.getByRole("button", { name: /Cheat codes/ }))
        expect(seen.scrolled > 0).toBe(true)
        expect(seen.focusedWhole).toBe(true)
        expect(seen.top).toBe("letters")
        expect(seen.bottom === "bad").toBe(false)
      })
    },
  },
  Narrow: {
    "the focus stands whole and neither edge is a letterless sliver": async ({ canvas, expect, waitFor }: CheckTools) => {
      await waitFor(() => {
        const seen = wedgeView(canvas.getByRole("button", { name: /Volume/ }))
        expect(seen.focusedWhole).toBe(true)
        expect(seen.top === "bad").toBe(false)
        expect(seen.bottom === "bad").toBe(false)
      })
    },
  },
}

/** Unexported: the real page on the real overlay model, a pretend Korri applying each value. */
function Scenario({
  overlay,
  art,
  focus,
  moving,
  asking = false,
}: {
  readonly overlay: SurfaceGameplayOverlayPresentation
  /** The running game's art, greyed behind the page. */
  readonly art?: string
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
  useFixtureDirections()
  return (
    <ZonePaused
      answer={model.answer}
      art={art}
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

/** Korri names the running game and its catalog has its cover: the game stands behind PAUSED in grey. */
export function OnTheGame() {
  return <Scenario art={ZONE_ART_BRAMBLE} overlay={overlayDefault} />
}

/** Only a cover, taller than wide: it fills the page in grey, cropped, never stretched. */
export function OnTheCover() {
  return <Scenario art={ZONE_ART_KNIGHT} focus="restart" overlay={overlayDestructive} />
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
