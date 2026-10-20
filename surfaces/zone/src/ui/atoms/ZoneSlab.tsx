import "./ZoneSlab.css"
import { useRef } from "react"
import { useZoneSlabShape } from "../../use-zone-slab-shape"
import { ZoneUnderline } from "./ZoneUnderline"

/**
 * A slab drawn behind a control's words: Mania's black 45 degree panel,
 * its edges stairs of one art px, filling the box of the positioned element
 * it stands in, and the pink drop under it when `lit`. Once its words take
 * more than one line it keeps the lean one line deep, a panel with its
 * leading corners cut (useZoneSlabShape).
 *
 * Unlike ZonePanel it takes no room and sets no padding: its parent lays
 * out the words and leaves the lean's room itself (the slab's height in
 * ems, known from the type), so measuring the slab never changes a size
 * and a fit that steps the type cannot set off a measuring loop. A parent
 * may start the slab lower than its words (`--zone-slab-top`, the caps
 * rising over its edge) or paint it another colour (`--zone-slab-colour`).
 */
export function ZoneSlab({ lit = false }: { readonly lit?: boolean }) {
  const slab = useRef<HTMLSpanElement>(null)
  useZoneSlabShape(slab)
  return (
    <span aria-hidden="true" className="zone-slab" ref={slab}>
      {lit ? <ZoneUnderline /> : null}
    </span>
  )
}
