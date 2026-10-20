import "./ZoneDialog.css"
import { useRef } from "react"
import { useZoneDialogMotion } from "../../use-zone-dialog-motion"
import { useZoneDialogShape } from "../../use-zone-dialog-shape"
import { useZoneFit } from "../../use-zone-fit"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZonePanel } from "../atoms/ZonePanel"
import { ZoneUnderline } from "../atoms/ZoneUnderline"
import { ZoneWords } from "../atoms/ZoneWords"

export interface ZoneDialogAction {
  readonly label: string
  readonly onPress?: () => void
  /** A stable key when labels may repeat (Korri's action id). Default: the label. */
  readonly id?: string
  /** Present but inert: Korri's rule is dimmed, never hidden. */
  readonly disabled?: boolean
  /** Destructive: the caps are in the danger ink. */
  readonly destructive?: boolean
}

/**
 * Mania's OPTIONS dialog: a sky panel (#31A2F7) cut by two black wedges,
 * top-left and bottom-right, their long edges 45 degree pixel stairs. The
 * headline is white caps-2; Korri's lines are Jersey 10 as written, white
 * with a black ring, at most about 52 characters a line; the game is the
 * HUD's yellow caps-1. The buttons are black 45 degree slabs in caps-2,
 * left to right in the order given, the lit one on the pink drop.
 *
 * The words are Korri's. The headline is its kicker, the lines are its
 * reason, and `title` is the game the trouble belongs to, when Korri names
 * one. The button labels are the page's, or Korri's own actions.
 *
 * Type steps, never shrinks: the headline and the buttons stand at caps-2
 * and step to caps-1 when their line is too narrow; only then does the
 * headline wrap between words and the buttons take a second row. The
 * dialog fills the width it is given and no more than the height; a long
 * body scrolls between the headline and the buttons, so the way out is
 * never below the fold. Focus is a prop; when the host moves DOM focus onto
 * a button, `onFocusAction` reports its index.
 *
 * M4: the panel opens from its middle in six steps while the wedges slide
 * in, and closes the same way faster (useZoneDialogMotion).
 */
export function ZoneDialog({
  headline,
  lines,
  title,
  actions,
  focused = 0,
  verbatimTitle = false,
  onFocusAction,
}: {
  readonly headline: string
  readonly lines: readonly string[]
  /** A third line in the HUD's yellow: the game's title. */
  readonly title?: string
  /** The buttons, left to right: NO then YES, or Korri's actions in its order. */
  readonly actions: readonly ZoneDialogAction[]
  /** The index of the focused button. */
  readonly focused?: number
  /** The yellow line is a fact Korri publishes that is not a title (a key): keep its case. */
  readonly verbatimTitle?: boolean
  /** The host's cursor arrived on a button: its index, left to right. */
  readonly onFocusAction?: (index: number) => void
}) {
  const panel = useRef<HTMLElement | null>(null)
  const head = useRef<HTMLHeadingElement | null>(null)
  const row = useRef<HTMLDivElement | null>(null)
  useZoneDialogShape(panel)
  useZoneDialogMotion(panel)
  useZoneFit(panel, [head, row], `${headline}|${actions.map(action => action.label).join("|")}`)
  return (
    <section aria-label={headline} className="zone-dialog" ref={panel} role="dialog">
      <span className="zone-dialog-wedge" data-wedge="start" />
      <span className="zone-dialog-wedge" data-wedge="end" />
      <h2 className="zone-dialog-headline" ref={head}>
        <ZoneWords>{headline}</ZoneWords>
      </h2>
      <div className="zone-dialog-body">
        {lines.map((line, index) => (
          <p className="zone-dialog-line" key={index}>{line}</p>
        ))}
        {title === undefined ? null : verbatimTitle ? (
          <p className="zone-dialog-title" data-case="verbatim">{title}</p>
        ) : (
          <p className="zone-dialog-title">
            <ZoneWords ink="hud">{title}</ZoneWords>
          </p>
        )}
      </div>
      <div className="zone-dialog-actions" ref={row}>
        {actions.map((action, index) => (
          <button
            className="zone-dialog-action"
            data-destructive={action.destructive || undefined}
            data-focused={index === focused || undefined}
            disabled={action.disabled}
            key={action.id ?? action.label}
            onClick={action.onPress}
            onFocus={onFocusAction === undefined ? undefined : () => onFocusAction(index)}
            type="button"
          >
            <ZonePanel edge={false}>
              <ZoneCaps ink={action.destructive ? "danger" : "white"}>{action.label}</ZoneCaps>
              {index === focused && !action.disabled ? <ZoneUnderline /> : null}
            </ZonePanel>
          </button>
        ))}
      </div>
    </section>
  )
}
