import "./ZonePanel.css"
import type { ReactNode } from "react"

/** What a panel is painted. Black is every panel in Zone but the dialog (blue), the HERE tag (yellow) and the keyboard's slab (white). */
export type ZonePanelTone = "black" | "blue" | "yellow" | "white"

/**
 * The parallelogram: Mania's one panel shape. The top edge leads the bottom
 * edge by `--zone-lean` times the type size, floored at `--zone-slant`, so a
 * tall panel leans as far as a short one does in proportion to its caps.
 *
 * The body is `::before` and the blue drop edge is `::after`, both the same
 * polygon; the children sit in front, and the underline, at z-index -2,
 * sits between the body and the edge.
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
  return (
    <div className="zone-panel" data-edge={edge ? "on" : "off"} data-tone={tone}>
      {children}
    </div>
  )
}
