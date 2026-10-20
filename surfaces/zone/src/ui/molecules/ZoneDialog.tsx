import "./ZoneDialog.css"

export interface ZoneDialogAction {
  readonly label: string
  readonly onPress?: () => void
}

/**
 * Mania's OPTIONS dialog: a blue panel cut by two black wedges, the
 * headline in caps, the body lines under it, and NO / YES as two black
 * parallelograms with the pink offset underline on the focused one.
 *
 * The words are Korri's. The headline is its kicker, the lines are its
 * reason, and `title` is the game the trouble belongs to, in yellow, when
 * Korri names one. The button labels are the page's.
 *
 * The dialog fills the width it is given and no more than the height; a
 * long body scrolls between the headline and the buttons, so the way out
 * is never below the fold. Focus is a prop.
 *
 * The button parallelograms and the underline are drawn here; they become
 * ZoneButton and ZoneUnderline after the merge.
 */
export function ZoneDialog({
  headline,
  lines,
  title,
  actions,
  focused = 0,
}: {
  readonly headline: string
  readonly lines: readonly string[]
  /** A third line in yellow: the game's title. */
  readonly title?: string
  /** One or two buttons, left to right: NO then YES. */
  readonly actions: readonly ZoneDialogAction[]
  /** The index of the focused button. */
  readonly focused?: number
}) {
  return (
    <section aria-label={headline} className="zone-dialog" role="dialog">
      <span className="zone-dialog-wedge zone-dialog-wedge-start" />
      <span className="zone-dialog-wedge zone-dialog-wedge-end" />
      <h2 className="zone-dialog-headline">{headline}</h2>
      <div className="zone-dialog-body">
        {lines.map((line, index) => (
          <p className="zone-dialog-line" key={index}>{line}</p>
        ))}
        {title !== undefined ? <p className="zone-dialog-title">{title}</p> : null}
      </div>
      <div className="zone-dialog-actions">
        {actions.map((action, index) => (
          <button
            className="zone-dialog-action"
            data-focused={index === focused || undefined}
            key={action.label}
            onClick={action.onPress}
            type="button"
          >
            {action.label}
          </button>
        ))}
      </div>
    </section>
  )
}
