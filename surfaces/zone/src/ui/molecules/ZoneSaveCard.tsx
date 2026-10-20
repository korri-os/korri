import "./ZoneSaveCard.css"
import type { ReactNode } from "react"
import { useRef } from "react"
import { useZoneFit } from "../../use-zone-fit"
import { ZoneCoverWell } from "../atoms/ZoneCoverWell"
import { ZoneTag, type ZoneTagItem } from "../atoms/ZoneTag"

/** What stands in the save card's well: art, a node (the QR code), or a line of text. */
export type ZoneSaveBody =
  | { readonly kind: "art"; readonly src: string }
  | { readonly kind: "node"; readonly node: ReactNode }
  /** `verbatim`: the text is a fact Korri publishes (a key) and keeps its case. */
  | { readonly kind: "text"; readonly text: string; readonly verbatim?: boolean }

/**
 * The vertical save card of MANIA MODE: the striped NEW-style header, a
 * white band with the header word in black caps across it, the dark well
 * below, and one tag at the foot. Focused, it wears the pink edge and a
 * red triangle above and below the well.
 *
 * A native button, like the zone card. Its height is the band's
 * `--zone-card-h` or, standing alone, the plan's focused height.
 */
export function ZoneSaveCard({
  header,
  body,
  tag,
  focused = false,
  onFocus,
}: {
  readonly header: string
  readonly body: ZoneSaveBody
  readonly tag?: ZoneTagItem
  readonly focused?: boolean
  readonly onFocus?: () => void
}) {
  const card = useRef<HTMLButtonElement>(null)
  const heading = useRef<HTMLSpanElement>(null)
  useZoneFit(card, [heading], header)
  return (
    <button
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
        <span className="zone-save-card-heading" ref={heading}>{header}</span>
      </span>
      <span className="zone-save-card-well">
        <i aria-hidden="true" className="zone-save-card-tri" data-side="up" />
        <i aria-hidden="true" className="zone-save-card-tri" data-side="down" />
        {body.kind === "art"
          ? <ZoneCoverWell art={body.src} title={header} />
          : (
            <ZoneCoverWell title={header}>
              {body.kind === "text"
                ? <span className="zone-save-card-text" data-case={body.verbatim === true ? "verbatim" : undefined}>{body.text}</span>
                : body.node}
            </ZoneCoverWell>
          )}
      </span>
      {tag === undefined ? null : (
        <span className="zone-save-card-foot">
          <ZoneTag kind={tag.kind} label={tag.label} />
        </span>
      )}
    </button>
  )
}
