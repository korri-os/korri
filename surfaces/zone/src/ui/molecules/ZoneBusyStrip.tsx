import "./ZoneBusyStrip.css"
import type { SurfaceAction } from "@contracts/surface/korri-surface"

/**
 * The action row while Korri works: one black parallelogram with its
 * kicker, the detail under it, and the host's actions (CANCEL) as
 * parallelogram buttons beside it, the focused one wearing the pink
 * underline.
 *
 * The kicker and the detail are Korri's words verbatim; the actions are
 * Korri's too, so cancellation is never inferred from a label. A disabled
 * action stays, dimmed. The strip takes the width it is given: the kicker
 * block yields first and wraps, the buttons keep their size, and when the
 * row is too narrow for both the buttons drop under the kicker.
 *
 * Focus is a prop; when the host moves DOM focus onto an action,
 * `onFocusAction` reports its index.
 *
 * The parallelograms and the underline are drawn here; they become
 * ZonePanel, ZoneButton and ZoneUnderline after the merge.
 */
export function ZoneBusyStrip({
  kicker,
  detail,
  actions = [],
  focused,
  onAction,
  onFocusAction,
}: {
  readonly kicker: string
  readonly detail?: string
  readonly actions?: readonly SurfaceAction[]
  /** The index of the focused action, if the cursor is on one. */
  readonly focused?: number
  readonly onAction?: (actionId: string) => void
  /** The host's cursor arrived on an action: its index, left to right. */
  readonly onFocusAction?: (index: number) => void
}) {
  return (
    <div aria-live="polite" className="zone-busy-strip" role="status">
      <div className="zone-busy-strip-panel">
        <span className="zone-busy-strip-kicker">{kicker}</span>
        {detail !== undefined ? <span className="zone-busy-strip-detail">{detail}</span> : null}
      </div>
      {actions.length > 0 ? (
        <div className="zone-busy-strip-actions">
          {actions.map((action, index) => (
            <button
              className="zone-busy-strip-action"
              data-focused={index === focused || undefined}
              data-destructive={action.destructive || undefined}
              disabled={!action.enabled}
              key={action.id}
              onClick={onAction === undefined ? undefined : () => onAction(action.id)}
              onFocus={onFocusAction === undefined ? undefined : () => onFocusAction(index)}
              type="button"
            >
              {action.label}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  )
}
