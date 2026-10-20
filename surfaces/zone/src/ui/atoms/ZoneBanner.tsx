import "./ZoneBanner.css"
import { useRef } from "react"
import { useZoneStairClip } from "../../use-zone-stair-clip"
import { ZoneCaps } from "./ZoneCaps"

/**
 * The title banner: Mania's blue band with a point at each end, its title
 * in the banner ink (yellow, a white band through rows 4 to 6, a black ring
 * and a 2-art-px orange drop) at caps-2, and the lip under it: 2 art px,
 * red on the left half and green on the right, with four notches at its
 * centre, two red and two green. Used once on a screen, for the current
 * tab. Sized by the type it stands in, times the banner scale (2), so the
 * caps-1 tabs beside it carry a caps-2 title.
 *
 * The points are 45 degree stairs from zoneStairClip, measured on the body
 * (useZoneStairClip); the lip hangs under the body and takes no room.
 */
export function ZoneBanner({ children }: { readonly children: string }) {
  const body = useRef<HTMLSpanElement>(null)
  useZoneStairClip(body, { left: "point", right: "point" })
  return (
    <span className="zone-banner">
      <span className="zone-banner-body" ref={body}>
        <span className="zone-banner-caps">
          <ZoneCaps ink="banner">{children}</ZoneCaps>
        </span>
      </span>
      <i className="zone-banner-lip" aria-hidden="true" />
    </span>
  )
}
