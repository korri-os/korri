/**
 * The size of the element Boxbuster is given, kept current.
 *
 * Layout is a function of this size and nothing else (boxbuster-layout.ts):
 * not the window, not the device. Before the element is measured, or where
 * ResizeObserver is missing, the size is zero, which lays out the counter
 * alone.
 */
import { type RefObject, useLayoutEffect, useState } from "react"

export interface BoxbusterSize {
  readonly width: number
  readonly height: number
}

const UNMEASURED: BoxbusterSize = { width: 0, height: 0 }

export function useBoxbusterSize(
  ref: RefObject<HTMLElement | null>,
): BoxbusterSize {
  const [size, setSize] = useState<BoxbusterSize>(UNMEASURED)

  useLayoutEffect(() => {
    const element = ref.current
    if (element === null) return
    const measure = () => {
      const { width, height } = element.getBoundingClientRect()
      const next = { width: Math.round(width), height: Math.round(height) }
      setSize(current =>
        current.width === next.width && current.height === next.height
          ? current
          : next,
      )
    }
    measure()
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [ref])

  return size
}
