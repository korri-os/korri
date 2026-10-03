/**
 * The unseen layer a controller moves through, over the store.
 *
 * Each target is a real button pinned over the thing it stands for: a tape,
 * a place on the floor to step to, a deck. The host moves focus between them
 * by screen position and turns confirm into a click. None of it is drawn: the
 * room shows what has focus (a tape slides out, a carpet mark glows, a deck
 * slot lights), so the buttons carry only focus and a spoken name.
 *
 * Focus: on arrival somewhere (a new spot, a tape picked up or put down, a
 * tape into or out of the deck, a launch failing), or when the focused button
 * goes away, focus lands where `landingFor` says. Focusing a step along a
 * shelf takes the step, so the d-pad glides down an aisle; a landing never
 * does, or focus would walk on its own.
 */
import "./BoxbusterTargets.css"
import { useLayoutEffect, useRef } from "react"
import type { SpotExit } from "./boxbuster-spots"
import type { TapeFacts } from "./boxbuster-store-view"
import type { Target } from "./boxbuster-targets"

export function BoxbusterTargets({
  targets,
  landing,
  arrival,
  tapes,
  onFocusChange,
  onTape,
  onExit,
  onDeck,
  onEject,
  onRetry,
}: {
  targets: readonly Target[]
  /** The key of the target focus should land on. */
  landing: string | undefined
  /** Changes whenever you arrive somewhere new, your hands or the deck
   * change, or a launch starts or stops failing. */
  arrival: string
  tapes: readonly TapeFacts[]
  /** The focused target's key, so the room can show it. */
  onFocusChange: (key: string | undefined) => void
  onTape: (tapeId: string) => void
  onExit: (exit: SpotExit) => void
  onDeck: (locationId?: string) => void
  onEject: () => void
  onRetry: () => void
}) {
  const layer = useRef<HTMLDivElement>(null)
  const landingFocus = useRef(false)
  const landedFor = useRef<string | undefined>(undefined)

  useLayoutEffect(() => {
    const root = layer.current
    if (root === null) return
    const arrived = landedFor.current !== arrival
    if (!arrived && !focusIsLost(root)) return
    const home =
      landing === undefined
        ? null
        : root.querySelector<HTMLElement>(
            `[data-target="${CSS.escape(landing)}"]`,
          )
    if (home === null) return
    landedFor.current = arrival
    landingFocus.current = true
    home.focus({ preventScroll: true })
    landingFocus.current = false
  })

  const titleOf = (id: string) =>
    tapes.find(tape => tape.id === id)?.title ?? id

  return (
    <div
      className="boxbuster-targets"
      ref={layer}
      onBlur={event => {
        if (!layer.current?.contains(event.relatedTarget as Node | null)) {
          onFocusChange(undefined)
        }
      }}
    >
      {targets.map(target => (
        <button
          key={target.key}
          type="button"
          className="boxbuster-target"
          data-target={target.key}
          data-kind={target._tag}
          aria-label={
            target._tag === "Tape"
              ? titleOf(target.tapeId)
              : target._tag === "Exit"
                ? target.exit.label
                : target.label
          }
          style={{ left: target.x, top: target.y }}
          onFocus={() => {
            onFocusChange(target.key)
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
              case "Retry":
                return onRetry()
            }
          }}
        />
      ))}
    </div>
  )
}

/** Focus is lost when nothing holds it, or what held it was removed. */
function focusIsLost(root: HTMLElement): boolean {
  const active = root.ownerDocument.activeElement
  return (
    active === null ||
    active === root.ownerDocument.body ||
    !active.isConnected
  )
}
