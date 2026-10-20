import "./ZoneBusyStrip.css"
import "../../zone-motion.css"
import type { SurfaceAction } from "@contracts/surface/korri-surface"
import { useLayoutEffect, useRef } from "react"
import { formatZoneBytes, zoneProgressShare, type ZoneProgress } from "../../plan/bytes"

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
 * With Korri's `progress` (C32) a meter stands under the detail, drawn as
 * SPECIAL STAGE's red slanted bar with the sizes at its right end: with a
 * total, the filled share and DONE / TOTAL; without one, the size done so
 * far and a stripe moving along the bar in place of the fill (still under
 * reduced motion, where it reads as a bar with no end). Nothing is done
 * yet and no end is known (unpacking): the stripe alone. Without
 * `progress` the strip says only that work is under way.
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
  progress,
  actions = [],
  focused,
  onAction,
  onFocusAction,
}: {
  readonly kicker: string
  readonly detail?: string
  /** Korri's measured progress, in bytes. */
  readonly progress?: ZoneProgress
  readonly actions?: readonly SurfaceAction[]
  /** The index of the focused action, if the cursor is on one. */
  readonly focused?: number
  readonly onAction?: (actionId: string) => void
  /** The host's cursor arrived on an action: its index, left to right. */
  readonly onFocusAction?: (index: number) => void
}) {
  const meter = useRef<HTMLDivElement>(null)
  const share = progress === undefined ? undefined : zoneProgressShare(progress)
  // The fill's share is a measurement written to the DOM, not state.
  useLayoutEffect(() => {
    meter.current?.style.setProperty("--zone-busy-strip-share", String(share ?? 0))
  }, [share])
  const done = progress === undefined ? undefined : formatZoneBytes(progress.done)
  const total = progress?.total === undefined ? undefined : formatZoneBytes(progress.total)
  // Unpacking: nothing done and no end known. A "0 B" would read as stuck.
  const figures = progress === undefined || (total === undefined && progress.done <= 0)
    ? undefined
    : total === undefined ? done : `${done} / ${total}`
  return (
    <div aria-live="polite" className="zone-busy-strip" role="status">
      <div className="zone-busy-strip-panel">
        <span className="zone-busy-strip-kicker">{kicker}</span>
        {detail !== undefined ? <span className="zone-busy-strip-detail">{detail}</span> : null}
        {progress === undefined ? null : (
          <div
            aria-label={kicker}
            aria-valuemax={share === undefined ? undefined : 100}
            aria-valuemin={share === undefined ? undefined : 0}
            aria-valuenow={share === undefined ? undefined : Math.floor(share * 100)}
            aria-valuetext={figures}
            className="zone-busy-strip-meter"
            data-meter={share === undefined ? "indeterminate" : "progress"}
            ref={meter}
            role="progressbar"
          >
            <span className="zone-busy-strip-bar">
              <span className="zone-busy-strip-fill" />
            </span>
            {figures === undefined ? null : (
              <span aria-hidden="true" className="zone-busy-strip-figures">
                <span className="zone-busy-strip-done">{done}</span>
                {total === undefined ? null : (
                  <>
                    <span className="zone-busy-strip-of">/</span>
                    <span className="zone-busy-strip-total">{total}</span>
                  </>
                )}
              </span>
            )}
          </div>
        )}
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
