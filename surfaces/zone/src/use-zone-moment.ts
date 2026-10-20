/**
 * The clock behind Zone's moments (REFINEMENT.md 2d, M1 to M6).
 *
 * A moment is a pure `frame(t, box)` from src/plan/motion.ts: given the
 * time since its trigger and its box in art px, the custom properties its
 * stylesheet reads. This hook runs that function on the screen's frames
 * from the moment `key` changes, writes the answer on the element, and
 * stops at `duration` (a loop keeps going; its frame wraps `t` itself).
 * `layout(box)` writes what depends on the box alone (a band's stair
 * polygon) once per size, not once per frame.
 *
 * Seekable: `window.seek(t)` holds every running moment at `t` ms after
 * its own trigger, the same frame every run, and `window.play()` lets them
 * run again from there. A take or a still reads a held frame.
 *
 * Under reduced motion a moment paints one frame, its `rest` (by default
 * its end), and runs no clock.
 *
 * Drawing, not state: nothing re-renders, as with use-zone-fit and the
 * ground's clock (the authoring gate names this file beside the ground's
 * as the only places that run a frame clock).
 */
import { useLayoutEffect, useRef, type RefObject } from "react"
import { zoneArtPxOf } from "./use-zone-plan"

export type ZoneMomentVars = Readonly<Record<string, string | number>>

export interface ZoneMomentBox {
  readonly wArt: number
  readonly hArt: number
}

export interface ZoneMoment {
  /** The trigger: the clock starts again at 0 whenever it changes. Undefined: no moment. */
  readonly key: string | undefined
  /** Milliseconds the moment runs; a loop runs until its key changes. */
  readonly duration: number
  readonly loop?: boolean
  /** The frame painted under reduced motion; the end by default. */
  readonly rest?: number
  readonly frame: (t: number, box: ZoneMomentBox) => ZoneMomentVars
  readonly layout?: (box: ZoneMomentBox) => ZoneMomentVars
}

interface Clock {
  paint(t: number): void
  play(): void
}

/** Every running moment, so `window.seek` can hold them all. */
const clocks = new Set<Clock>()
let held: number | undefined

/** Hold every moment at `t` ms after its own trigger. */
export function seekZoneMoments(t: number): void {
  held = t
  for (const clock of clocks) clock.paint(t)
}

/** Let every moment run again from where it is held. */
export function playZoneMoments(): void {
  held = undefined
  for (const clock of clocks) clock.play()
}

interface SeekableWindow {
  seek?: (t: number) => void
  play?: () => void
}

if (typeof window !== "undefined") {
  const target = window as unknown as SeekableWindow
  target.seek = seekZoneMoments
  target.play = playZoneMoments
}

const reducedMotion = (): boolean =>
  typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches

const write = (element: HTMLElement, vars: ZoneMomentVars) => {
  for (const [name, value] of Object.entries(vars)) element.style.setProperty(name, String(value))
}

export function useZoneMoment(element: RefObject<HTMLElement | null>, moment: ZoneMoment): void {
  // The latest frame and layout, so a running clock paints the props of now.
  const latest = useRef(moment)
  latest.current = moment
  const { key } = moment
  useLayoutEffect(() => {
    const box = element.current
    if (box === null || key === undefined) return
    let size: ZoneMomentBox = { wArt: 0, hArt: 0 }
    let now = 0
    let request = 0
    const measure = (): boolean => {
      const px = zoneArtPxOf(box)
      const next = { wArt: Math.floor(box.clientWidth / px), hArt: Math.floor(box.clientHeight / px) }
      if (next.wArt === size.wArt && next.hArt === size.hArt) return false
      size = next
      const layout = latest.current.layout
      if (layout !== undefined) write(box, layout(size))
      return true
    }
    const paint = (t: number) => {
      now = t
      write(box, latest.current.frame(t, size))
    }
    const still = reducedMotion()
    const started = performance.now()
    const tick = () => {
      const { duration, loop = false } = latest.current
      const t = performance.now() - started
      paint(loop ? t : Math.min(t, duration))
      if (loop || t < duration) request = requestAnimationFrame(tick)
    }
    const clock: Clock = {
      paint: t => {
        cancelAnimationFrame(request)
        paint(t)
      },
      play: () => {
        cancelAnimationFrame(request)
        if (still) return
        const from = now
        const resumed = performance.now() - from
        const run = () => {
          const { duration, loop = false } = latest.current
          const t = performance.now() - resumed
          paint(loop ? t : Math.min(t, duration))
          if (loop || t < duration) request = requestAnimationFrame(run)
        }
        run()
      },
    }
    measure()
    if (held !== undefined) paint(held)
    else if (still) paint(latest.current.rest ?? latest.current.duration)
    else tick()
    clocks.add(clock)
    const observer = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(() => {
      if (measure()) paint(now)
    })
    observer?.observe(box)
    return () => {
      clocks.delete(clock)
      cancelAnimationFrame(request)
      observer?.disconnect()
    }
  }, [element, key])
}
