import { useEffect, useRef, useState } from "react"
import { type PicoOverlayRange, picoRangeStep } from "./pico-overlay-view"

/** How long a step with no release edge waits before Korri is asked. */
const COMMIT_DELAY_MS = 180
/** How long a held direction waits for its release before Korri is asked anyway. */
const RELEASE_FALLBACK_MS = 2000

/** One step request: which way, and whether the input will report a release. */
export interface PicoRangeStepRequest {
  readonly way: -1 | 1
  readonly releaseExpected: boolean
  readonly gestureId?: number
  readonly source?: string
}

/** The release of a held direction, matched against the step it ended. */
export interface PicoRangeRelease {
  readonly way: -1 | 1
  readonly gestureId?: number
  readonly source?: string
}

/**
 * A range's value while the user adjusts it, and when Korri is told.
 *
 * The value moves on every step, so the screen answers the thumb at once. Korri
 * is asked once per gesture: when a held direction is released, or shortly
 * after the last step when the input reports no release. A held d-pad repeats
 * many times a second, and asking Korri on every repeat would queue a stale
 * value behind each fresh one. The timings and the release matching are
 * Shift's (surfaces/shift/src/ui/molecules/ShiftSheetRange.tsx), so both
 * surfaces answer the same input the same way.
 *
 * A value Korri republishes replaces the local one and drops any step not yet
 * sent.
 */
export function usePicoRange(
  range: PicoOverlayRange | undefined,
  enabled: boolean,
  commit: (value: number) => void,
) {
  const [value, setValue] = useState(range?.value)
  const valueRef = useRef(range?.value)
  const commitRef = useRef(commit)
  const pending = useRef<{ readonly value: number; readonly request: PicoRangeStepRequest } | null>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const stopTimer = () => {
    if (timer.current !== null) clearTimeout(timer.current)
    timer.current = null
  }
  const flush = () => {
    stopTimer()
    const waiting = pending.current
    pending.current = null
    if (waiting !== null) commitRef.current(waiting.value)
  }
  const cancel = () => {
    stopTimer()
    pending.current = null
  }

  useEffect(() => {
    commitRef.current = commit
  }, [commit])

  useEffect(() => {
    cancel()
    valueRef.current = range?.value
    setValue(range?.value)
  }, [range?.value, range?.min, range?.max, range?.step, enabled])

  useEffect(() => () => cancel(), [])

  const step = (request: PicoRangeStepRequest) => {
    const current = valueRef.current
    if (!enabled || range === undefined || current === undefined) return
    const next = picoRangeStep({ ...range, value: current }, request.way)
    if (next === current) return
    valueRef.current = next
    setValue(next)
    stopTimer()
    pending.current = { value: next, request }
    timer.current = setTimeout(flush, request.releaseExpected ? RELEASE_FALLBACK_MS : COMMIT_DELAY_MS)
  }

  const release = (ended: PicoRangeRelease) => {
    const request = pending.current?.request
    if (request === undefined) return
    if (request.way !== ended.way || request.gestureId !== ended.gestureId || request.source !== ended.source) return
    flush()
  }

  return { value, step, release, flush }
}
