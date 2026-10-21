/**
 * The frame every save list shares (split review, save history).
 *
 * It is Settings' scene: the home's backdrop and status bar, a kicker and
 * title, a list that moves under a fixed reading band on the rail's spring,
 * and a footer with the host's actions and the legend. The list never scrolls
 * natively; the focused row is kept in the band, so directional input always
 * brings it into view, and rows cut off at the band's edges show that more
 * exists.
 *
 * `state` replaces the list for the loading, empty and error states, so those
 * keep the same head and footer and the person can always leave.
 */
import type { SurfaceAction } from "@contracts/surface/korri-surface"
import { MotionConfig, motion } from "framer-motion"
import { type ReactNode, useEffect, useRef, useState } from "react"
import type { ShiftDesignPart } from "../../shift-design-parts"
import { shiftDesignPartAttrs } from "../../shift-design-parts"
import { ShiftCineKicker } from "../atoms/ShiftCineKicker"
import { ShiftCineTitle } from "../atoms/ShiftCineTitle"
import { ShiftCineBackdrop } from "../molecules/ShiftCineBackdrop"
import { type ShiftCineHintSpec, ShiftCineLegend } from "../molecules/ShiftCineLegend"
import { ShiftStatusBar } from "../molecules/ShiftStatusBar"

/** Same spring as the rail and Settings. */
const SPRING = { type: "spring", stiffness: 260, damping: 32 } as const

export interface ShiftSavesFrameProps {
  readonly part: ShiftDesignPart
  readonly kicker: string
  readonly title: string
  readonly lede?: string
  readonly time?: string
  readonly backdropArtUrl?: string
  /** Absolute index of the focused row, for the reading band. */
  readonly focusedIndex: number
  /** Loading, empty or error content in place of the list. */
  readonly state?: ReactNode
  readonly children?: ReactNode
  /** Host actions shown as footer buttons, ids unchanged. */
  readonly actions?: readonly SurfaceAction[]
  readonly onAction?: (actionId: string) => void
  readonly onActionFocus?: () => void
  readonly hints: readonly ShiftCineHintSpec[]
  readonly attrs?: Readonly<Record<string, string>>
}

export function ShiftSavesFrame({
  part,
  kicker,
  title,
  lede,
  time,
  backdropArtUrl,
  focusedIndex,
  state,
  children,
  actions = [],
  onAction,
  onActionFocus,
  hints,
  attrs,
}: ShiftSavesFrameProps) {
  const viewportRef = useRef<HTMLDivElement>(null)
  const trackRef = useRef<HTMLDivElement>(null)
  const [trackY, setTrackY] = useState(0)

  useEffect(() => {
    const recenter = () => {
      const track = trackRef.current
      const viewport = viewportRef.current
      if (!track || !viewport) return
      const row = track.querySelector<HTMLElement>(
        `[data-save-index="${focusedIndex}"]`,
      )
      if (!row) {
        setTrackY(0)
        return
      }
      const wanted =
        viewport.clientHeight / 2 - (row.offsetTop + row.offsetHeight / 2)
      const min = Math.min(0, viewport.clientHeight - track.scrollHeight)
      setTrackY(Math.max(min, Math.min(0, wanted)))
    }
    recenter()
    const observer = new ResizeObserver(recenter)
    if (viewportRef.current) observer.observe(viewportRef.current)
    return () => observer.disconnect()
  }, [focusedIndex, state])

  // Seed focus on the first row, or on the first footer action when the list
  // has none, so directional input always has a starting point.
  const seeded = useRef(false)
  useEffect(() => {
    if (seeded.current) return
    const first =
      trackRef.current?.querySelector<HTMLElement>('[data-save-index="0"]') ??
      viewportRef.current?.parentElement?.querySelector<HTMLElement>(
        ".shift-saves-action:not([disabled])",
      )
    if (!first) return
    seeded.current = true
    first.focus({ preventScroll: true })
  })

  return (
    <MotionConfig reducedMotion="user">
    <div
      data-shift-saves
      className="shift-cine intrinsic relative h-full w-full overflow-hidden"
      {...shiftDesignPartAttrs(part)}
      {...attrs}
    >
      <ShiftCineBackdrop artUrl={backdropArtUrl ?? ""} cooled />
      <ShiftStatusBar time={time} />

      <div className="shift-saves-stage">
        <header className="shift-saves-head">
          <ShiftCineKicker>{kicker}</ShiftCineKicker>
          <ShiftCineTitle>{title}</ShiftCineTitle>
          {lede ? <p className="shift-saves-lede">{lede}</p> : null}
        </header>

        {/* The band moves the list. A focus call that scrolls the clipped
            viewport natively would add a second offset and empty the band,
            so native scrolling is undone at once. */}
        <div
          className="shift-saves-viewport"
          ref={viewportRef}
          onScroll={event => {
            event.currentTarget.scrollTop = 0
          }}
        >
          {state ?? (
            <motion.div
              className="shift-saves-track"
              ref={trackRef}
              animate={{ y: trackY }}
              transition={SPRING}
            >
              {children}
            </motion.div>
          )}
        </div>

        <footer className="shift-saves-footer">
          <div className="shift-saves-actions">
            {actions.map(action => (
              <button
                key={action.id}
                type="button"
                className="shift-saves-action"
                data-tone={action.destructive ? "danger" : undefined}
                disabled={!action.enabled}
                onFocus={onActionFocus}
                onClick={() => onAction?.(action.id)}
              >
                {action.label}
              </button>
            ))}
          </div>
          {hints.length > 0 ? <ShiftCineLegend hints={hints} /> : null}
        </footer>
      </div>
    </div>
    </MotionConfig>
  )
}

/** The state block shown in place of a list: a short headline and one line. */
export function ShiftSavesState({
  tone,
  headline,
  line,
  loading,
}: {
  readonly tone?: "danger"
  readonly headline: string
  readonly line?: string
  readonly loading?: boolean
}) {
  return (
    <div
      className="shift-saves-state"
      data-tone={tone}
      role={tone === "danger" ? "alert" : "status"}
    >
      <p className="shift-saves-state-headline">{headline}</p>
      {line ? <p className="shift-saves-state-line">{line}</p> : null}
      {loading ? <span className="shift-saves-loading" aria-hidden /> : null}
    </div>
  )
}
