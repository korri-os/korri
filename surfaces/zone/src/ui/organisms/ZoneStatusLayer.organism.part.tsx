import type { SurfaceStatus } from "@contracts/surface/korri-surface"
import { type ReactNode, useRef, useSyncExternalStore } from "react"
import {
  fixtureTitleOf,
  statusBrowsing,
  statusBusyCancel,
  statusBusyDetail,
  statusBusyKicker,
  statusBusyTwo,
  statusConfirm,
  statusConfirmForGame,
  statusConfirmThree,
  statusProblem,
  statusProblemLong,
  statusProblemNoRetry,
  statusProgressKnown,
  statusRunning,
} from "../../fixtures/fixture-layers"
import { useFixtureTake } from "../../fixtures/fixture-take"
import { createStatusProgram } from "../../fixtures/layer-program"
import { fixtureCards } from "../../fixtures/zone-cards"
import { ZoneKeycap } from "../atoms/ZoneKeycap"
import { ZoneCardBand } from "../molecules/ZoneCardBand"
import { ZoneTabStrip } from "../molecules/ZoneTabStrip"
import { ZoneShell } from "../templates/ZoneShell"
import { ZoneGround } from "./ZoneGround"
import { ZoneStatusLayer } from "./ZoneStatusLayer"

export const name = "Status layer"
export const note = "Korri's status over any page: the busy strip, the running strip, the problem dialog, Korri's question"

/** Unexported: the real layer on the real status model, a pretend Korri answering. */
function Scenario({ status, page, stage }: { readonly status: SurfaceStatus; readonly page?: ReactNode; readonly stage?: ReactNode }) {
  const program = useRef<ReturnType<typeof createStatusProgram> | null>(null)
  program.current ??= createStatusProgram(status)
  const { dispatch, subscribe, get } = program.current
  const { model, context } = useSyncExternalStore(subscribe, get)
  return (
    <ZoneStatusLayer
      focus={model.focus}
      onBusyAction={(actionId) => dispatch({ _tag: "ChoseBusyAction", actionId })}
      onConfirmAction={(actionId) => dispatch({ _tag: "ChoseConfirmAction", actionId })}
      onFocusButton={(index) => dispatch({ _tag: "FocusedStatusButton", index })}
      onProblemButton={(button) => dispatch({ _tag: "ChoseProblemButton", button })}
      page={page}
      stage={stage}
      status={context}
      titleOf={fixtureTitleOf}
    />
  )
}

/* A stand-in for the page under the layer: the ground and the card band.
 * Not a component: a fixture node. */
const standInPage = (
  <>
    <ZoneGround />
    <ZoneCardBand cards={fixtureCards} focusedIndex={3} />
  </>
)

/* A stand-in with the shell's top strip, so the running HUD finds the
 * page's header and stands under it. Not a component: a fixture node. */
const tabItems = [
  { id: "games", label: "Games" },
  { id: "options", label: "Options" },
  { id: "savedata", label: "Save data" },
]
const shellPage = (
  <>
    <ZoneGround />
    <ZoneShell
      hintBack={<ZoneKeycap button="B" word="Back" />}
      hintConfirm={<ZoneKeycap button="A" word="Confirm" />}
      tabs={<ZoneTabStrip active="games" onSelect={() => undefined} tabs={tabItems} />}
    >
      <ZoneCardBand cards={fixtureCards} focusedIndex={3} />
    </ZoneShell>
  </>
)

/* A stand-in for the launch title card: the title card's flat yellow. */
const titleStage = <ZoneGround ground="titlecard" />

/** Korri's problem, the game in yellow, DISMISS lit before RETRY. */
export default function Default() {
  return <Scenario page={standInPage} status={statusProblem} />
}

/** Browsing: the layer draws nothing over the page. */
export function Browsing() {
  return <Scenario page={standInPage} status={statusBrowsing} />
}

/** Busy with Korri's kicker alone: no game, no detail, nothing to press. */
export function BusyKicker() {
  return <Scenario status={statusBusyKicker} />
}

/** Busy with the detail under the kicker and the game above it. */
export function BusyDetail() {
  return <Scenario status={statusBusyDetail} />
}

/** Korri's CANCEL beside the strip, lit; focus is in the strip. */
export function BusyWithCancel() {
  return <Scenario status={statusBusyCancel} />
}

/** Two host actions, CANCEL and the destructive STOP. */
export function BusyTwoActions() {
  return <Scenario status={statusBusyTwo} />
}

/** A game is live: the small strip at the top of the page, which stays usable. */
export function Running() {
  return <Scenario page={standInPage} status={statusRunning} />
}

/** The dialog with DISMISS and RETRY. */
export function Problem() {
  return <Scenario status={statusProblem} />
}

/** Korri cannot retry: DISMISS alone. */
export function ProblemNoRetry() {
  return <Scenario status={statusProblemNoRetry} />
}

/** A long kicker, a long reason with one unbroken word and the longest title: the body scrolls, the buttons stay. */
export function ProblemLongReason() {
  return <Scenario status={statusProblemLong} />
}

/** Busy over the games page: the page dims and goes inert under the strip. */
export function OverPage() {
  return <Scenario page={standInPage} status={statusBusyCancel} />
}

/** Korri's question: its kicker and detail, one button per host action, the first lit. */
export function Confirm() {
  return <Scenario page={standInPage} status={statusConfirm} />
}

/** Three host actions in the host's order; the long one wraps the row before any button shrinks. */
export function ConfirmThreeActions() {
  return <Scenario page={standInPage} status={statusConfirmThree} />
}

/** The question belongs to one game: its title in yellow under the detail. */
export function ConfirmForGame() {
  return <Scenario page={standInPage} status={statusConfirmForGame} />
}

/** A game is live over a page with the shell: the HUD stands at the left under the top strip, clear of B BACK and the tabs. */
export function RunningUnderHeader() {
  return <Scenario page={shellPage} status={statusRunning} />
}

/** A launch under the title card (the stage): the busy strip stands along its foot. */
export function BusyUnderStage() {
  return <Scenario page={standInPage} stage={titleStage} status={statusBusyDetail} />
}

/** Korri asks while the title card stands: the question covers it. */
export function ConfirmOverStage() {
  return <Scenario page={standInPage} stage={titleStage} status={statusConfirmForGame} />
}

/** Korri's download with a known end over the page: the red bar under the detail, CANCEL lit. */
export function BusyProgress() {
  return <Scenario page={standInPage} status={statusProgressKnown} />
}

/**
 * M4's take: T opens Korri's problem over the page and shuts it again, so
 * the dialog plays its open (six steps, 180 ms) and its close (140 ms);
 * `window.seek(ms)` holds a frame.
 */
export function OpenAndClose() {
  const open = useFixtureTake()
  return <ZoneStatusLayer page={standInPage} status={open ? statusProblem : statusBrowsing} titleOf={fixtureTitleOf} />
}
