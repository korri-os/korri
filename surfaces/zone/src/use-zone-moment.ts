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
 * Seekable: `window.seek(t)` holds every running moment, and the ground's
 * drift, at `t` ms after its own start, the same frame every run, and
 * `window.play()` lets them run again from there. A take or a still reads
 * a held frame.
 *
 * Under reduced motion a moment paints one frame, its `rest` (by default
 * its end), and runs no clock.
 *
 * Drawing, not state: nothing re-renders, as with use-zone-fit and the
 * ground's clock (the authoring gate names this file beside the ground's
 * as the only places that run a frame clock).
 */
import { useLayoutEffect, useRef, type RefObject } from "react"
import type { ZoneLookDrawing } from "./plan/look"
import { useZoneLookKey, zoneDrawingOf } from "./use-zone-look"
import { zoneArtPxOf } from "./use-zone-plan"

export type ZoneMomentVars = Readonly<Record<string, string | number>>

export interface ZoneMomentBox {
  readonly wArt: number
  readonly hArt: number
  /** How the look the box stands in draws: its edges and its motion grid. */
  readonly drawing: ZoneLookDrawing
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

export interface ZoneClockTiming {
  /** Milliseconds the clock runs; a loop runs until it is stopped. */
  readonly duration: number
  readonly loop?: boolean
  /** The time painted under reduced motion. */
  readonly rest: number
}

export interface ZoneClockRun {
  /** Paint again at the time it stands at (a resize). */
  readonly repaint: () => void
  readonly stop: () => void
}

/**
 * The one seekable clock: run `paint(t)` on the screen's frames from now,
 * `t` the ms since the start, until `timing().duration` (a loop keeps
 * going). While `window.seek` holds the moments it paints the held time
 * and runs no frames; `window.play()` runs it on from there. Under reduced
 * motion it paints `rest` once. Every moment and the ground's drift run on
 * it, so one seek freezes the whole screen. The only place in Zone that
 * reads the time (test/authoring-gate.test.ts).
 */
export function startZoneClock(paint: (t: number) => void, timing: () => ZoneClockTiming): ZoneClockRun {
  let now = 0
  let request = 0
  const still = reducedMotion()
  const show = (t: number) => {
    now = t
    paint(t)
  }
  const runFrom = (from: number) => {
    cancelAnimationFrame(request)
    const origin = performance.now() - from
    const run = () => {
      const { duration, loop = false } = timing()
      const t = performance.now() - origin
      show(loop ? t : Math.min(t, duration))
      if (loop || t < duration) request = requestAnimationFrame(run)
    }
    run()
  }
  const clock: Clock = {
    paint: t => {
      cancelAnimationFrame(request)
      show(t)
    },
    play: () => {
      cancelAnimationFrame(request)
      if (!still) runFrom(now)
    },
  }
  if (held !== undefined) show(held)
  else if (still) show(timing().rest)
  else runFrom(0)
  clocks.add(clock)
  return {
    repaint: () => paint(now),
    stop: () => {
      clocks.delete(clock)
      cancelAnimationFrame(request)
    },
  }
}

export function useZoneMoment(element: RefObject<HTMLElement | null>, moment: ZoneMoment): void {
  // The latest frame and layout, so a running clock paints the props of now.
  const latest = useRef(moment)
  latest.current = moment
  const { key } = moment
  // A look changes the drawing (stairs or smooth, art px or free): measure again.
  const look = useZoneLookKey()
  useLayoutEffect(() => {
    const box = element.current
    if (box === null || key === undefined) return
    let size: ZoneMomentBox = { wArt: 0, hArt: 0, drawing: zoneDrawingOf(box) }
    const measure = (): boolean => {
      const px = zoneArtPxOf(box)
      const next = { wArt: Math.floor(box.clientWidth / px), hArt: Math.floor(box.clientHeight / px), drawing: zoneDrawingOf(box) }
      if (next.wArt === size.wArt && next.hArt === size.hArt && next.drawing.edges === size.drawing.edges
        && next.drawing.motion === size.drawing.motion) return false
      size = next
      const layout = latest.current.layout
      if (layout !== undefined) write(box, layout(size))
      return true
    }
    measure()
    const clock = startZoneClock(t => write(box, latest.current.frame(t, size)), () => ({
      duration: latest.current.duration,
      loop: latest.current.loop ?? false,
      rest: latest.current.rest ?? latest.current.duration,
    }))
    const observer = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(() => {
      if (measure()) clock.repaint()
    })
    observer?.observe(box)
    return () => {
      clock.stop()
      observer?.disconnect()
    }
  }, [element, key, look])
}

/**
 * M2's trigger. A tab chosen (ZoneTab) marks the moment with the colour of
 * the ground it leaves; the shell of the page that arrives takes the mark
 * when it mounts (a new page) or when its own page changes (the same shell)
 * and plays the menu transition from that colour. A mark older than the
 * transition is stale and plays nothing, so a page that mounts for any
 * other reason (power-on, a part) arrives still.
 */
let transition: { readonly at: number; readonly ground: string } | undefined

export function markZoneTransition(from: Element | null): void {
  const ground = from === null ? "" : getComputedStyle(from).getPropertyValue("--zone-ground").trim()
  transition = { at: performance.now(), ground }
}

/** The ground colour a fresh tab change left, once; undefined when there is none. */
export function takeZoneTransition(within: number): string | undefined {
  const mark = transition
  transition = undefined
  return mark !== undefined && performance.now() - mark.at <= within ? mark.ground : undefined
}
