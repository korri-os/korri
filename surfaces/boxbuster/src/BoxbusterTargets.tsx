/**
 * The layer over the store that a controller moves through.
 *
 * Each target is a real button pinned over the thing it stands for: a tape
 * on a shelf, a way on, a deck. The host moves focus between them by screen
 * position and turns confirm into a click. Nothing here is a menu: the only
 * text is a label on what you look at, a sign on a way on, and Korri's words
 * printed on the TV.
 *
 * Focus: on arrival somewhere (a new spot, a tape picked up or put down, a
 * tape into or out of the deck), or when the focused button goes away, focus
 * lands where `landingFor` says. Focusing a step along a shelf takes the step,
 * so the d-pad glides down an aisle; a landing never does, or focus would walk
 * on its own.
 */
import "./BoxbusterTargets.css"
import { useLayoutEffect, useRef, useState } from "react"
import type { SpotExit } from "./boxbuster-spots"
import type { TapeFacts, TvStatus } from "./boxbuster-store-view"
import type { ScreenRect, Target } from "./boxbuster-targets"

export function BoxbusterTargets({
  targets,
  landing,
  arrival,
  tapes,
  heldId,
  tvFrame,
  status,
  onTape,
  onExit,
  onDeck,
  onEject,
  onRetry,
  onDismiss,
}: {
  targets: readonly Target[]
  /** The key of the target focus should land on. */
  landing: string | undefined
  /** Changes whenever you arrive somewhere new, your hands or the deck
   * change, or the TV starts saying something else. */
  arrival: string
  tapes: readonly TapeFacts[]
  heldId: string | undefined
  /** The TV on screen, when you stand in front of it. */
  tvFrame: ScreenRect | undefined
  status: TvStatus
  onTape: (tapeId: string) => void
  onExit: (exit: SpotExit) => void
  onDeck: (locationId?: string) => void
  onEject: () => void
  onRetry: () => void
  onDismiss: () => void
}) {
  const layer = useRef<HTMLDivElement>(null)
  const landingFocus = useRef(false)
  const landedFor = useRef<string | undefined>(undefined)
  const [lookingAt, setLookingAt] = useState<string | undefined>(undefined)
  const problem = tvFrame !== undefined && status._tag === "Problem"

  useLayoutEffect(() => {
    const root = layer.current
    if (root === null) return
    const arrived = landedFor.current !== arrival
    if (!arrived && !focusIsLost(root)) return
    const home = problem
      ? root.querySelector<HTMLElement>("[data-tv-action]")
      : landing === undefined
        ? null
        : root.querySelector<HTMLElement>(`[data-target="${CSS.escape(landing)}"]`)
    if (home === null) return
    landedFor.current = arrival
    landingFocus.current = true
    home.focus({ preventScroll: true })
    landingFocus.current = false
  })

  const titled = (id: string | undefined) =>
    id === undefined ? undefined : tapes.find(tape => tape.id === id)
  // A tape you looked at before you picked something up is not on screen now.
  const looked = targets.some(t => t._tag === "Tape" && t.tapeId === lookingAt)
    ? titled(lookingAt)
    : undefined
  const label = looked ?? titled(heldId)

  return (
    <div className="boxbuster-targets" ref={layer}>
      {targets.map(target => (
        <BoxbusterTarget
          key={target.key}
          target={target}
          title={target._tag === "Tape" ? titled(target.tapeId)?.title : undefined}
          onFocus={() => {
            setLookingAt(target._tag === "Tape" ? target.tapeId : undefined)
            if (
              target._tag === "Exit" &&
              target.exit.glide &&
              !landingFocus.current
            ) {
              onExit(target.exit)
            }
          }}
          onClick={() => {
            switch (target._tag) {
              case "Tape":
                return onTape(target.tapeId)
              case "Exit":
                return onExit(target.exit)
              case "Deck":
                return onDeck(target.locationId)
              case "Eject":
                return onEject()
            }
          }}
        />
      ))}

      {label === undefined ? null : <BoxbusterTapeLabel tape={label} />}

      {tvFrame === undefined ? null : (
        <BoxbusterTvText
          frame={tvFrame}
          status={status}
          onRetry={onRetry}
          onDismiss={onDismiss}
        />
      )}
    </div>
  )
}

function BoxbusterTarget({
  target,
  title,
  onFocus,
  onClick,
}: {
  target: Target
  title: string | undefined
  onFocus: () => void
  onClick: () => void
}) {
  const label =
    target._tag === "Tape"
      ? (title ?? target.tapeId)
      : target._tag === "Exit"
        ? target.exit.label
        : target.label
  return (
    <button
      type="button"
      className="boxbuster-target"
      data-target={target.key}
      data-kind={target._tag}
      data-align={target.align}
      aria-label={label}
      style={{ left: target.x, top: target.y }}
      onFocus={onFocus}
      onClick={onClick}
    >
      {target._tag === "Tape" ? null : (
        <span className="boxbuster-target-sign">{label}</span>
      )}
    </button>
  )
}

/** The readable label on the tape you look at, or the one in your hand. */
function BoxbusterTapeLabel({ tape }: { tape: TapeFacts }) {
  return (
    <div className="boxbuster-tape-label" aria-live="polite">
      <p className="boxbuster-tape-label-title">{tape.title}</p>
      {tape.subtitle === undefined ? null : (
        <p className="boxbuster-tape-label-line">{tape.subtitle}</p>
      )}
      {tape.facts.map(fact => (
        <p key={fact} className="boxbuster-tape-label-line">
          {fact}
        </p>
      ))}
    </div>
  )
}

/** Korri's words about a launch, printed on the TV. */
function BoxbusterTvText({
  frame,
  status,
  onRetry,
  onDismiss,
}: {
  frame: ScreenRect
  status: TvStatus
  onRetry: () => void
  onDismiss: () => void
}) {
  if (status._tag === "Idle") return null
  return (
    <div
      className="boxbuster-tv"
      data-status={status._tag}
      role={status._tag === "Problem" ? "alert" : "status"}
      style={{
        left: frame.left,
        top: frame.top,
        width: frame.width,
        height: frame.height,
      }}
    >
      <p className="boxbuster-tv-kicker">{status.kicker}</p>
      {status._tag === "Working" && status.detail !== undefined ? (
        <p className="boxbuster-tv-line">{status.detail}</p>
      ) : null}
      {status._tag === "Problem" ? (
        <>
          {status.title === undefined ? null : (
            <p className="boxbuster-tv-line">{status.title}</p>
          )}
          <p className="boxbuster-tv-line">{status.reason}</p>
          <div className="boxbuster-tv-actions">
            {status.canRetry ? (
              <button
                type="button"
                className="boxbuster-tv-action"
                data-tv-action=""
                onClick={onRetry}
              >
                Try again
              </button>
            ) : null}
            <button
              type="button"
              className="boxbuster-tv-action"
              {...(status.canRetry ? {} : { "data-tv-action": "" })}
              onClick={onDismiss}
            >
              Back to the store
            </button>
          </div>
        </>
      ) : null}
    </div>
  )
}

/** Focus is lost when nothing holds it, or what held it was removed. */
function focusIsLost(root: HTMLElement): boolean {
  const active = root.ownerDocument.activeElement
  return active === null || active === root.ownerDocument.body || !active.isConnected
}
