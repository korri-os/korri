import type { ReactNode } from "react"

/**
 * What a card says by its colour: `ask` is a question (yellow), `warn` is
 * something that went wrong or cannot be undone (red), `tell` is information
 * (blue).
 */
export type PicoCardTone = "ask" | "warn" | "tell"

/**
 * A card shaped like a cartridge: a notched plastic shell with a hard shadow,
 * holding a kicker, a title and whatever the card is for.
 *
 * Questions, failures and menus all arrive as one of these, so the colour is
 * the first thing a player reads. Rows inside a card take their colours from
 * the card, which is how the same row reads on yellow, red and blue.
 */
export function PicoCard({
  tone,
  shell,
  kicker,
  title,
  titleId,
  children,
}: {
  readonly tone: PicoCardTone
  /** Paint the card in a cartridge plastic (an index into the shell
   * colours) instead of its tone's colour, so a card can match the spine or
   * cart it belongs to. The tone still says what the card is. */
  readonly shell?: 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9
  readonly kicker?: string
  readonly title?: string
  /** Lets a dialog name itself after the card's title. */
  readonly titleId?: string
  readonly children?: ReactNode
}) {
  return (
    <div className="pico-card" data-shell={shell} data-tone={tone}>
      {kicker === undefined ? null : <span className="pico-card-kicker">{kicker}</span>}
      {title === undefined ? null : (
        <h2 className="pico-card-title" id={titleId}>
          {title}
        </h2>
      )}
      {children}
    </div>
  )
}
