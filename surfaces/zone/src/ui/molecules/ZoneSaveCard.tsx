import "./ZoneSaveCard.css"
import type { ReactNode } from "react"
import { Fragment, useRef } from "react"
import { useZoneFit } from "../../use-zone-fit"
import { ZoneCaps, type ZoneInk } from "../atoms/ZoneCaps"
import { ZoneCoverWell } from "../atoms/ZoneCoverWell"
import { ZoneEmblem, type ZoneEmblemName } from "../atoms/ZoneEmblem"
import type { ZoneTagItem, ZoneTagKind } from "../atoms/ZoneTag"

/** What stands in the save card's window: art, a node, or Zone's emblem over a word. */
export type ZoneSaveBody =
  | { readonly kind: "art"; readonly src: string }
  | { readonly kind: "node"; readonly node: ReactNode }
  /**
   * The window's word on a black tag, under an emblem when one is named.
   * `verbatim`: the word is a fact Korri publishes (a key) and keeps its
   * case, in Jersey 10.
   */
  | { readonly kind: "text"; readonly text: string; readonly verbatim?: boolean; readonly emblem?: ZoneEmblemName }

/** The foot bar's ink: black on the light colours, white on the dark. */
const BAR_INK: Readonly<Record<ZoneTagKind, ZoneInk>> = {
  system: "white",
  here: "ink",
  remote: "white",
  saved: "ink",
  open: "ink",
  continue: "ink",
  download: "ink",
}

/** A shortened key may break after its ellipsis, and nowhere else. */
function wordOf(text: string, verbatim: boolean): ReactNode {
  const cut = text.indexOf("…")
  if (!verbatim || cut < 0) return text
  return [text.slice(0, cut + 1), <wbr key="cut" />, text.slice(cut + 1)].map((part, index) => <Fragment key={index}>{part}</Fragment>)
}

/**
 * MANIA MODE's save card. A black frame round three parts:
 *
 *  - the header: pale teal with a yellow stripe over a white bar and a red
 *    and a blue stripe under it; the label in black caps on the bar, caps-2
 *    stepping to caps-1 to fit;
 *  - the window: pale teal with a teal diagonal, Zone's own emblem centred
 *    in it and the window's word under it on a black tag (caps-1, wrapping
 *    between words; a key in Jersey 10, breaking only after its ellipsis);
 *  - the foot: a status, when there is one, as a bar across the card's
 *    whole width in the status's colour.
 *
 * Focused, the card wears the pink edge and a red triangle over and under
 * its window. A native button, like the zone card. Its height is the
 * band's `--zone-card-h` or, standing alone, the plan's focused height.
 */
export function ZoneSaveCard({
  header,
  body,
  tag,
  focused = false,
  label,
  onFocus,
}: {
  readonly header: string
  readonly body: ZoneSaveBody
  /** A status for the foot bar. */
  readonly tag?: ZoneTagItem
  readonly focused?: boolean
  /** The card's name for a reader, when the words on it say less. */
  readonly label?: string
  readonly onFocus?: () => void
}) {
  const card = useRef<HTMLButtonElement>(null)
  const heading = useRef<HTMLSpanElement>(null)
  useZoneFit(card, [heading], header)
  return (
    <button
      aria-label={label}
      className="zone-save-card"
      data-focused={focused ? "true" : undefined}
      onFocus={onFocus}
      ref={card}
      type="button"
    >
      <span className="zone-save-card-header">
        <i aria-hidden="true" className="zone-save-card-stripe" data-stripe="upper" />
        <i aria-hidden="true" className="zone-save-card-stripe" data-stripe="lower" />
        <i aria-hidden="true" className="zone-save-card-stripe" data-stripe="lowest" />
        <span className="zone-save-card-heading" ref={heading}>
          <ZoneCaps ink="ink">{header}</ZoneCaps>
        </span>
      </span>
      <span className="zone-save-card-well" data-kind={body.kind}>
        <i aria-hidden="true" className="zone-save-card-tri" data-side="up" />
        <i aria-hidden="true" className="zone-save-card-tri" data-side="down" />
        {body.kind === "art" ? <ZoneCoverWell art={body.src} title={header} /> : null}
        {body.kind === "node" ? <ZoneCoverWell title={header}>{body.node}</ZoneCoverWell> : null}
        {body.kind === "text" ? (
          <span className="zone-save-card-window">
            {body.emblem === undefined ? null : (
              <span className="zone-save-card-emblem">
                <ZoneEmblem name={body.emblem} />
              </span>
            )}
            <span className="zone-save-card-tag">
              <span className="zone-save-card-text" data-case={body.verbatim === true ? "verbatim" : undefined}>
                {wordOf(body.text, body.verbatim === true)}
              </span>
            </span>
          </span>
        ) : null}
      </span>
      {tag === undefined ? null : (
        <span className="zone-save-card-foot" data-kind={tag.kind}>
          <ZoneCaps ink={BAR_INK[tag.kind]} size="caps-1">{tag.label}</ZoneCaps>
        </span>
      )}
    </button>
  )
}
