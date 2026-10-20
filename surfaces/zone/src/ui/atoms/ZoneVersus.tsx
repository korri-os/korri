import "./ZoneVersus.css"
import { useRef } from "react"
import { useZoneStairClip } from "../../use-zone-stair-clip"
import { ZoneCaps } from "./ZoneCaps"

/**
 * COMPETITION's VS: a black panel leaning at 45 degrees, its two ends
 * pixel stairs, VS across it in white caps-2. Zone's own word between two
 * seats; read as it is written.
 */
export function ZoneVersus() {
  const panel = useRef<HTMLSpanElement>(null)
  useZoneStairClip(panel, { left: "slash", right: "slash" })
  return (
    <span className="zone-versus" ref={panel}>
      <ZoneCaps size="caps-2">VS</ZoneCaps>
    </span>
  )
}
