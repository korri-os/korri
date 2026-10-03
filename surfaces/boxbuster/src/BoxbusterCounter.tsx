/**
 * The counter: where every decision in Boxbuster happens.
 *
 * The store is the place; the counter is the legible way through it, drawn
 * from the same tapes with readable type and no dither. It holds the tape in
 * front of you or in your hand, where a launch stands, and the index of every
 * tape. When the store cannot be drawn, the counter is the whole surface.
 *
 * Focus: each state marks one control `data-focus-home`, the place a confirm
 * should land. When the focused control goes away (a tape is put down, a
 * problem clears), focus returns home, so a controller is never left pointing
 * at nothing.
 */
import "./BoxbusterCounter.css"
import { useEffect, useRef } from "react"
import { BoxbusterCounterIndex } from "./BoxbusterCounterIndex"
import {
  BoxbusterCounterProblem,
  BoxbusterCounterStatusLine,
} from "./BoxbusterCounterStatus"
import {
  BoxbusterCounterTape,
  BoxbusterCounterTapeInHand,
} from "./BoxbusterCounterTape"
import type { BoxbusterHand } from "./boxbuster-hand"
import type { CounterStatus, CounterTape } from "./boxbuster-store-view"

export function BoxbusterCounter({
  tapes,
  hand,
  status,
  onLook,
  onPickUp,
  onPutDown,
  onPlay,
  onRetry,
  onDismiss,
}: {
  tapes: readonly CounterTape[]
  hand: BoxbusterHand
  status: CounterStatus
  onLook: (tapeId: string) => void
  onPickUp: (tapeId: string) => void
  onPutDown: () => void
  onPlay: (tapeId: string, locationId?: string) => void
  onRetry: () => void
  onDismiss: () => void
}) {
  const root = useRef<HTMLElement>(null)
  const tape =
    hand._tag === "NoTape"
      ? undefined
      : tapes.find(candidate => candidate.id === hand.tapeId)

  useEffect(() => {
    const counter = root.current
    if (counter === null) return
    if (!focusIsLost()) return
    const home = Array.from(
      counter.querySelectorAll<HTMLElement>("[data-focus-home]"),
    ).find(element => element.closest("[inert]") === null)
    home?.focus()
  })

  return (
    <aside className="boxbuster-counter" aria-label="Counter" ref={root}>
      <div className="boxbuster-counter-desk">
        <div className="boxbuster-counter-front">
          <BoxbusterCounterStatusLine status={status} />
          {status._tag === "Problem" ? (
            <BoxbusterCounterProblem
              problem={status}
              onRetry={onRetry}
              onDismiss={onDismiss}
            />
          ) : tape === undefined ? null : hand._tag === "Holding" ? (
            <BoxbusterCounterTapeInHand
              tape={tape}
              onPlay={locationId => onPlay(tape.id, locationId)}
              onPutDown={onPutDown}
            />
          ) : (
            <BoxbusterCounterTape tape={tape} />
          )}
        </div>
        <BoxbusterCounterIndex
          tapes={tapes}
          standingAt={tape?.id}
          inert={hand._tag === "Holding" || status._tag === "Problem"}
          onLook={onLook}
          onPickUp={onPickUp}
        />
      </div>
    </aside>
  )
}

/**
 * Focus is lost when nothing holds it, or what holds it can no longer be
 * reached. Focus somewhere else that is live is left alone.
 */
function focusIsLost(): boolean {
  const active = document.activeElement
  if (active === null || active === document.body) return true
  if (!active.isConnected) return true
  return active.closest("[inert]") !== null
}
