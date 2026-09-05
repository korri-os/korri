import { useEffect, useLayoutEffect, useRef, useState } from "react"
import type { PicoShelfGame } from "../../pico-shelf-game"
import { PicoTally } from "../atoms/PicoTally"
import { PicoCart } from "../molecules/PicoCart"
import { PicoKeyArtStage } from "../molecules/PicoKeyArtStage"

/**
 * The shelf: every game as a cartridge, the focused one held in the middle.
 *
 * Focus is the selection. The shelf tracks which cart has it so the hero and
 * the caption follow the d-pad, and it centres that cart itself rather than
 * leaving it to the browser's focus scrolling — which does nothing at all
 * before the first focus, so the shelf would otherwise open with its hero
 * sitting off to one side.
 *
 * The index is view state, not app state: nothing outside this section needs to
 * know where the cursor is, and hoisting it would make every neighbour
 * re-render on a movement that concerns only the shelf.
 */
export function PicoCartShelf({
  games,
  onOpen,
}: {
  readonly games: readonly PicoShelfGame[]
  readonly onOpen: (gameId: string) => void
}) {
  const [focusedIndex, setFocusedIndex] = useState(0)
  const [subtitleOverflows, setSubtitleOverflows] = useState(false)
  const stripRef = useRef<HTMLUListElement>(null)
  const subtitleRef = useRef<HTMLParagraphElement>(null)
  const focused = games[focusedIndex] ?? games[0]

  useEffect(() => {
    const strip = stripRef.current
    const slot = strip?.children[focusedIndex]
    if (strip === null || !(slot instanceof HTMLElement)) return
    /* Sets the strip's own scroll rather than calling scrollIntoView, which
     * walks up and moves every scrollable ancestor with it — a surface that can
     * scroll the page it is embedded in is a surface that misbehaves inside a
     * gallery or a host that stacks it with anything else. */
    strip.scrollLeft =
      slot.offsetLeft - (strip.clientWidth - slot.clientWidth) / 2
  }, [focusedIndex])

  /* Scroll a caption only when it actually does not fit. An unconditional
   * marquee makes every short subtitle move for no reason, which is noise
   * rather than character. Measured after layout because it depends on the
   * rendered width, not on the string. */
  useLayoutEffect(() => {
    const node = subtitleRef.current
    if (node === null) return
    setSubtitleOverflows(node.scrollWidth > node.clientWidth + 1)
  }, [focused?.subtitle])

  return (
    <section className="pico-cart-shelf">
      <PicoKeyArtStage src={focused?.wideArtUrl} />
      <ul className="pico-cart-shelf-strip" ref={stripRef}>
        {games.map((game, index) => (
          <li className="pico-cart-shelf-slot" key={game.id}>
            <PicoCart
              artUrl={game.artUrl}
              id={game.id}
              onActivate={() => onOpen(game.id)}
              onFocus={() => setFocusedIndex(index)}
              placement={index === focusedIndex ? "hero" : "side"}
              resumable={game.resumable ?? false}
              subtitle={game.subtitle}
              title={game.title}
            />
          </li>
        ))}
      </ul>
      <div className="pico-cart-shelf-caption">
        <h1 className="pico-cart-shelf-title">{focused?.title ?? ""}</h1>
        <p
          className="pico-cart-shelf-subtitle"
          data-marquee={subtitleOverflows ? "" : undefined}
          ref={subtitleRef}
        >
          <span className="pico-cart-shelf-subtitle-run">
            {focused?.subtitle ?? ""}
          </span>
        </p>
        <PicoTally position={focusedIndex + 1} total={games.length} />
      </div>
    </section>
  )
}
