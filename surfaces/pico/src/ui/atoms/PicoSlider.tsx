import "../../pico-motion.css"
import "./PicoSlider.css"
import { useEffect, useRef } from "react"
import type { PicoRangeRelease, PicoRangeStepRequest } from "../../pico-overlay-view"

/* The portal delivers left and right to the focused element marked
 * data-korri-horizontal-control as these DOM events instead of moving focus
 * (clients/portal/src/input/spatial-focus.ts). The surface never sees a key. */
const DIRECTION_EVENT = "korri-semantic-direction"
const DIRECTION_END_EVENT = "korri-semantic-direction-end"

/** A meter longer than this draws this many cells; each cell is then several steps. */
const MAX_CELLS = 20

interface DirectionDetail {
  readonly direction?: string
  readonly releaseExpected?: boolean
  readonly gestureId?: number
  readonly source?: string
}

const wayOf = (direction: string | undefined): -1 | 1 | undefined =>
  direction === "left" ? -1 : direction === "right" ? 1 : undefined

/**
 * A menu line that holds a number: its label on the left, and on the right ◀,
 * a meter of whole cells, the value, and ▶.
 *
 * One focus stop. Up and down pass over it like any row; left and right step
 * it. The arrows dim at min and max, so the end of the range is visible before
 * the press that does nothing. A pointer can press the arrows. Confirm does
 * nothing: there is no single right answer to "press Volume".
 *
 * A disabled slider is not focusable, the same as a disabled row, and stays in
 * the list so Korri's reason beneath it has something to explain.
 */
export function PicoSlider({
  label,
  value,
  min,
  max,
  step,
  valueText,
  disabled = false,
  onStep,
  onRelease,
  onLeave,
}: {
  readonly label: string
  readonly value: number
  readonly min: number
  readonly max: number
  readonly step: number
  /** The value as the screen states it, to the step's precision. */
  readonly valueText: string
  readonly disabled?: boolean
  readonly onStep: (request: PicoRangeStepRequest) => void
  readonly onRelease: (ended: PicoRangeRelease) => void
  /** Focus left the slider: anything not yet sent should be sent now. */
  readonly onLeave: () => void
}) {
  const root = useRef<HTMLDivElement>(null)
  const handlers = useRef({ onStep, onRelease })
  useEffect(() => {
    handlers.current = { onStep, onRelease }
  })

  useEffect(() => {
    const node = root.current
    if (node === null) return
    const onDirection = (event: Event) => {
      const detail = (event as CustomEvent<DirectionDetail>).detail
      const way = wayOf(detail?.direction)
      if (way === undefined) return
      handlers.current.onStep({
        way,
        releaseExpected: detail.releaseExpected === true,
        ...(detail.gestureId === undefined ? {} : { gestureId: detail.gestureId }),
        ...(detail.source === undefined ? {} : { source: detail.source }),
      })
    }
    const onDirectionEnd = (event: Event) => {
      const detail = (event as CustomEvent<DirectionDetail>).detail
      const way = wayOf(detail?.direction)
      if (way === undefined) return
      handlers.current.onRelease({
        way,
        ...(detail.gestureId === undefined ? {} : { gestureId: detail.gestureId }),
        ...(detail.source === undefined ? {} : { source: detail.source }),
      })
    }
    node.addEventListener(DIRECTION_EVENT, onDirection)
    node.addEventListener(DIRECTION_END_EVENT, onDirectionEnd)
    return () => {
      node.removeEventListener(DIRECTION_EVENT, onDirection)
      node.removeEventListener(DIRECTION_END_EVENT, onDirectionEnd)
    }
  }, [])

  const span = max - min
  const cells = Math.max(1, Math.min(MAX_CELLS, Math.round(span / step)))
  const lit = span <= 0 ? cells : Math.round(((value - min) / span) * cells)
  const press = (way: -1 | 1) => () => onStep({ way, releaseExpected: false })

  return (
    <div
      aria-disabled={disabled ? "true" : undefined}
      aria-label={label}
      aria-valuemax={max}
      aria-valuemin={min}
      aria-valuenow={value}
      aria-valuetext={valueText}
      className="pico-slider"
      data-korri-horizontal-control={disabled ? undefined : "range"}
      onBlur={onLeave}
      ref={root}
      role="slider"
      tabIndex={disabled ? undefined : 0}
    >
      <span className="pico-slider-label">{label}</span>
      <span className="pico-slider-control">
        <span
          className="pico-slider-arrow"
          data-off={value <= min ? "true" : undefined}
          data-step="down"
          onClick={disabled ? undefined : press(-1)}
        >
          ◀
        </span>
        <span className="pico-slider-meter">
          {Array.from({ length: cells }, (_, index) => (
            <span className="pico-slider-cell" data-lit={index < lit ? "true" : undefined} key={index} />
          ))}
        </span>
        <span className="pico-slider-value">{valueText}</span>
        <span
          className="pico-slider-arrow"
          data-off={value >= max ? "true" : undefined}
          data-step="up"
          onClick={disabled ? undefined : press(1)}
        >
          ▶
        </span>
      </span>
    </div>
  )
}
