import "./ZoneBusyStrip.css"
import "../../zone-motion.css"
import type { SurfaceAction } from "@contracts/surface/korri-surface"
import { useLayoutEffect, useRef } from "react"
import { formatZoneBytes, zoneProgressShare, type ZoneProgress } from "../../plan/bytes"
import { useZoneBusyShape } from "../../use-zone-busy-shape"
import { useZoneFit } from "../../use-zone-fit"
import { useZoneStairClip } from "../../use-zone-stair-clip"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZonePanel } from "../atoms/ZonePanel"
import { ZoneUnderline } from "../atoms/ZoneUnderline"
import { ZoneWords } from "../atoms/ZoneWords"

/**
 * The strip while Korri works: one black panel, its top-left and
 * bottom-right corners cut by 45 degree stairs (a parallelogram when it is
 * one line tall), with Korri's kicker in white caps-2 and the detail under
 * it in Jersey 10 as written; the host's actions (CANCEL) stand beside it
 * as black slabs in caps-2, the lit one on the pink drop.
 *
 * The kicker and the detail are Korri's words; the actions are Korri's
 * too, so cancellation is never inferred from a label. A disabled action
 * stays, dimmed; a destructive one is in the danger ink. Type steps, never
 * shrinks: the kicker and the actions stand at caps-2 and step to caps-1
 * when the strip is narrow; only then does the kicker wrap between words
 * and the actions drop under the panel.
 *
 * With Korri's `progress` (C32) a meter stands under the detail: SPECIAL
 * STAGE's bar, 4 art px tall with 45 degree stair ends, a red fill under a
 * 1-px orange lip on a dark track, and the sizes in white caps-1 at its
 * right end: with a total, the filled share and DONE / TOTAL; without one,
 * the size so far and red stripes sliding along the whole bar (still under
 * reduced motion, where it reads as a bar with no end). Nothing done and no
 * end known (unpacking): the stripes alone. Without `progress` the strip
 * says only that work is under way.
 *
 * The strip takes the width and the place its parent gives it, so a page,
 * the status layer or the launch title card can stand it where it belongs.
 * Focus is a prop; when the host moves DOM focus onto an action,
 * `onFocusAction` reports its index.
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
  const strip = useRef<HTMLDivElement>(null)
  const panel = useRef<HTMLDivElement>(null)
  const head = useRef<HTMLSpanElement>(null)
  const row = useRef<HTMLDivElement>(null)
  const meter = useRef<HTMLDivElement>(null)
  const bar = useRef<HTMLSpanElement>(null)
  const fill = useRef<HTMLSpanElement>(null)
  useZoneBusyShape(panel)
  useZoneStairClip(bar)
  useZoneStairClip(fill, { left: "straight" })
  useZoneFit(strip, [head, row], `${kicker}|${actions.map(action => action.label).join("|")}`)
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
    <div aria-live="polite" className="zone-busy-strip" ref={strip} role="status">
      <div className="zone-busy-strip-panel" ref={panel}>
        <span className="zone-busy-strip-kicker" ref={head}>
          <ZoneWords>{kicker}</ZoneWords>
        </span>
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
            <span className="zone-busy-strip-bar" ref={bar}>
              <span className="zone-busy-strip-fill" ref={fill} />
            </span>
            {figures === undefined || done === undefined ? null : (
              <span aria-hidden="true" className="zone-busy-strip-figures">
                <span className="zone-busy-strip-done"><ZoneCaps size="caps-1">{done}</ZoneCaps></span>
                {total === undefined ? null : (
                  <>
                    <span className="zone-busy-strip-of"><ZoneCaps size="caps-1">/</ZoneCaps></span>
                    <span className="zone-busy-strip-total"><ZoneCaps size="caps-1">{total}</ZoneCaps></span>
                  </>
                )}
              </span>
            )}
          </div>
        )}
      </div>
      {actions.length > 0 ? (
        <div className="zone-busy-strip-actions" ref={row}>
          {actions.map((action, index) => (
            <button
              className="zone-busy-strip-action"
              data-destructive={action.destructive || undefined}
              data-focused={index === focused || undefined}
              disabled={!action.enabled}
              key={action.id}
              onClick={onAction === undefined ? undefined : () => onAction(action.id)}
              onFocus={onFocusAction === undefined ? undefined : () => onFocusAction(index)}
              type="button"
            >
              <ZonePanel edge={false}>
                <ZoneCaps ink={action.destructive ? "danger" : "white"}>{action.label}</ZoneCaps>
                {index === focused && action.enabled ? <ZoneUnderline /> : null}
              </ZonePanel>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  )
}
