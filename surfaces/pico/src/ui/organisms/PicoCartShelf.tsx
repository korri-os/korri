import "./PicoCartShelf.css"
import { useEffect, useRef, useState } from "react"
import { picoStatsFor } from "../../pico-detail-view"
import type { PicoShelfGame } from "../../pico-shelf-game"
import { PicoTally } from "../atoms/PicoTally"
import { PicoCart } from "../molecules/PicoCart"
import { PicoGameStage } from "./PicoGameStage"

/**
 * The shelf: every game as a small cartridge standing on one line, and the
 * chosen one large on the stage above it.
 *
 * Carts on the shelf all have the same width and take their height from their
 * art, so a tall box stands tall and a wide header lies low, all on one
 * baseline. Focus is the selection: the stage follows the d-pad, and the
 * shelf scrolls only as far as it must to keep the chosen cart in view.
 *
 * The chosen cart is the shelf's own state. An owner that unmounts the shelf
 * and wants the same cart back later passes `selectedId` in and keeps what
 * `onSelect` reports; the shelf reads `selectedId` when it mounts.
 *
 * The cursor coming onto the shelf from anywhere else lands on the chosen
 * cart, not on whichever cart is nearest. Up from a control below the shelf,
 * or the first press on a screen with nothing focused, otherwise picks a cart
 * by position and moves the stage away from the game the player left it on.
 * A pointer chooses the cart it touches.
 */
export function PicoCartShelf({
  games,
  selectedId,
  onSelect,
  onOpen,
}: {
  readonly games: readonly PicoShelfGame[]
  readonly selectedId?: string
  readonly onSelect?: (gameId: string) => void
  readonly onOpen: (gameId: string) => void
}) {
  const [chosenId, setChosenId] = useState(selectedId)
  const rackRef = useRef<HTMLUListElement>(null)
  /* Whether the cursor is on this shelf already, and whether the next focus
   * comes from a pointer. Refs, because both are read inside a focus event and
   * neither draws anything. */
  const holding = useRef(false)
  const pointing = useRef(false)
  const found = games.findIndex((game) => game.id === chosenId)
  const focusedIndex = found === -1 ? 0 : found
  const focused = games[focusedIndex]

  const choose = (index: number) => {
    const game = games[index]
    if (game === undefined) return
    setChosenId(game.id)
    onSelect?.(game.id)
  }

  /* Sets the rack's own scroll rather than calling scrollIntoView, which walks
   * up and moves every scrollable ancestor with it — a surface that can scroll
   * the page it is embedded in misbehaves inside any host that stacks it with
   * anything else. One cart of margin either side, so the neighbour shows
   * there is more. */
  const reveal = (index: number) => {
    const rack = rackRef.current
    const slot = rack?.children[index]
    if (rack === null || rack === undefined || !(slot instanceof HTMLElement)) return
    const margin = slot.offsetWidth
    const left = slot.offsetLeft - rack.scrollLeft
    const right = left + slot.offsetWidth
    if (right > rack.clientWidth - margin) {
      rack.scrollLeft += right - (rack.clientWidth - margin)
    } else if (left < margin) {
      rack.scrollLeft -= margin - left
    }
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: The rack follows the chosen cart only.
  useEffect(() => reveal(focusedIndex), [focusedIndex])

  /* Any focus that lands off the shelf means the cursor has left it. */
  useEffect(() => {
    const note = (event: FocusEvent) => {
      if (event.target instanceof Node && rackRef.current?.contains(event.target) === true) return
      holding.current = false
      pointing.current = false
    }
    document.addEventListener("focusin", note)
    return () => document.removeEventListener("focusin", note)
  }, [])

  if (focused === undefined) return null

  return (
    <section className="pico-cart-shelf">
      <div className="pico-cart-shelf-stage">
        <PicoGameStage
          artUrl={focused.artUrl}
          id={focused.id}
          kicker={focused.section?.toUpperCase()}
          resumable={focused.resumable}
          stats={picoStatsFor(focused)}
          subtitle={focused.subtitle}
          title={focused.title}
        >
          <span className="pico-cart-shelf-tally">
            <PicoTally position={focusedIndex + 1} total={games.length} />
          </span>
        </PicoGameStage>
      </div>
      <ul
        aria-label="Shelf"
        className="pico-cart-shelf-rack"
        onFocusCapture={(event) => {
          const entering = !holding.current && !pointing.current
          const target: EventTarget = event.target
          holding.current = true
          pointing.current = false
          // A focus event with no focus behind it moves nothing.
          if (!entering || document.activeElement !== target) return
          const chosen = event.currentTarget.children[focusedIndex]?.querySelector("button")
          if (chosen === null || chosen === undefined || chosen === target) return
          event.stopPropagation()
          chosen.focus({ preventScroll: true })
          // The host reveals the cart it picked once focus returns; follow it.
          queueMicrotask(() => reveal(focusedIndex))
        }}
        onPointerDownCapture={() => { pointing.current = true }}
        ref={rackRef}
      >
        {games.map((game, index) => (
          <li className="pico-cart-shelf-slot" key={game.id}>
            <PicoCart
              artUrl={game.artUrl}
              id={game.id}
              onActivate={() => onOpen(game.id)}
              onFocus={() => choose(index)}
              placement={index === focusedIndex ? "hero" : "side"}
              progress={
                game.resumable === true
                  ? "resume"
                  : picoStatsFor(game).length === 0
                    ? "new"
                    : "played"
              }
              subtitle={game.subtitle}
              title={game.title}
            />
          </li>
        ))}
      </ul>
    </section>
  )
}
