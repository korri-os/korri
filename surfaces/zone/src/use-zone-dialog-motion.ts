/**
 * M4 on a dialog: open when it mounts, close when it unmounts
 * (sr./plan/dialog.ts holds the frames).
 *
 * Open: the panel and its two wedges play `zoneDialogKeyframes(wArt,
 * "open")` through the Web Animations API, each frame held until the next,
 * so the browser shows exactly the plan's frames and any of them can be
 * held (`animation.currentTime = t`).
 *
 * Close: React removes a dialog the moment Korri moves on, so there is
 * nothing left to animate. Just before it goes, this copies the panel as it
 * stands into the screen (`.zone-screen`) at the same place, hidden from
 * assistive technology, inert and deaf to the pointer, plays the close
 * frames on the copy and removes it. The copy is drawing, not state:
 * nothing re-renders and nothing can focus it.
 *
 * Neither runs under `prefers-reduced-motion` (the end state shows at
 * once) or on a box that was never laid out (a test DOM), where there is no
 * width to travel.
 */
import { useLayoutEffect, type RefObject } from "react"
import { zoneDialogDuration, zoneDialogEasing, zoneDialogKeyframes, type ZoneDialogPhase } from "./plan/dialog"
import { zoneDrawingOf } from "./use-zone-look"
import { zoneArtPxOf } from "./use-zone-plan"

const stillMotion = (): boolean =>
  typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches

/** Play `phase` on a panel and its wedges; the animations, or none when it cannot move. */
export function playZoneDialog(panel: HTMLElement, phase: ZoneDialogPhase): readonly Animation[] {
  if (stillMotion() || typeof panel.animate !== "function") return []
  const width = panel.getBoundingClientRect().width
  if (!(width > 0)) return []
  const px = zoneArtPxOf(panel)
  const { motion } = zoneDrawingOf(panel)
  const frames = zoneDialogKeyframes(Math.round(width / px), phase, motion)
  const timing = { duration: zoneDialogDuration(phase), fill: "both" as const }
  const held = { easing: zoneDialogEasing(phase, motion) }
  const played = [
    panel.animate(frames.map(frame => ({ ...held, offset: frame.offset, scale: `1 ${frame.scaleY}` })), timing),
  ]
  panel.querySelectorAll<HTMLElement>("[data-wedge]").forEach(wedge => {
    const sign = wedge.dataset.wedge === "start" ? -1 : 1
    played.push(wedge.animate(
      frames.map(frame => ({ ...held, offset: frame.offset, translate: `${sign * frame.wedgeArt * px}px 0` })),
      timing,
    ))
  })
  return played
}

/** A copy of the panel, left where it stood, playing the close; removed when it ends. */
function leave(panel: HTMLElement): void {
  if (stillMotion() || typeof panel.animate !== "function") return
  const rect = panel.getBoundingClientRect()
  if (!(rect.width > 0) || !panel.isConnected) return
  const stage = panel.closest<HTMLElement>(".zone-screen") ?? panel.ownerDocument.body
  const frame = stage.getBoundingClientRect()
  const copy = panel.cloneNode(true) as HTMLElement
  copy.removeAttribute("role")
  copy.removeAttribute("aria-label")
  copy.setAttribute("aria-hidden", "true")
  copy.inert = true
  copy.dataset.leaving = ""
  copy.style.setProperty("--zone-dialog-leave-x", `${rect.left - frame.left - stage.clientLeft + stage.scrollLeft}px`)
  copy.style.setProperty("--zone-dialog-leave-y", `${rect.top - frame.top - stage.clientTop + stage.scrollTop}px`)
  copy.style.setProperty("--zone-dialog-leave-w", `${rect.width}px`)
  copy.style.setProperty("--zone-dialog-leave-h", `${rect.height}px`)
  stage.append(copy)
  const played = playZoneDialog(copy, "close")
  let gone = false
  const remove = () => {
    if (gone) return
    gone = true
    copy.remove()
  }
  if (played.length === 0) return remove()
  void Promise.all(played.map(animation => animation.finished)).then(remove, remove)
  setTimeout(remove, zoneDialogDuration("close") * 2)
}

export function useZoneDialogMotion(panel: RefObject<HTMLElement | null>): void {
  useLayoutEffect(() => {
    const element = panel.current
    if (element === null) return
    playZoneDialog(element, "open")
    return () => leave(element)
  }, [panel])
}
