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
  statusRunning,
} from "../../fixtures/fixture-layers"
import { createStatusProgram } from "../../fixtures/layer-program"
import { fixtureCards } from "../../fixtures/zone-cards"
import { ZoneCardBand } from "../molecules/ZoneCardBand"
import { ZoneGround } from "./ZoneGround"
import { ZoneStatusLayer } from "./ZoneStatusLayer"

export const name = "Status layer"
export const note = "Korri's status over any page: the busy strip, the running strip, the problem dialog, Korri's question"

/** Unexported: the real layer on the real status model, a pretend Korri answering. */
function Scenario({ status, page }: { readonly status: SurfaceStatus; readonly page?: ReactNode }) {
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
