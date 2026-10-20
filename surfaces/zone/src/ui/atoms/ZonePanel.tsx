import "./ZonePanel.css"
import { useRef, type ReactNode } from "react"
import { useZoneStairClip } from "../../use-zone-stair-clip"

/** What a panel is painted. Black is every panel in Zone but the dialog (blue), the HERE tag (yellow) and the keyboard's slab (white). */
export type ZonePanelTone = "black" | "blue" | "yellow" | "white"

/**
 * The parallelogram: Mania's one panel shape, leaning at exactly 45 degrees
 * (its slant is its height) with stair edges, one step per art px, from
 * `zoneStairClip` (src/plan/stair-clip.ts). The panel measures its own
 * height in art px to know how many stairs it has (useZoneStairClip).
 *
 * The body is `::before` and the drop edge (the ground's shade, two art px
 * down and right) is `::after`, both the same polygon; the children sit in
 * front, and the underline, at z-index -2, sits between the body and the
 * edge.
 */
export function ZonePanel({
  tone = "black",
  edge = true,
  children,
}: {
  readonly tone?: ZonePanelTone
  /** The hard blue offset behind the panel. Off for a tab or a tag that sits shoulder to shoulder with another. */
  readonly edge?: boolean
  readonly children: ReactNode
}) {
  const panel = useRef<HTMLDivElement>(null)
  useZoneStairClip(panel)
  return (
    <div className="zone-panel" data-edge={edge ? "on" : "off"} data-tone={tone} ref={panel}>
      {children}
    </div>
  )
}
