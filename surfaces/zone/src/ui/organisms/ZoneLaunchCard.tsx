import "./ZoneLaunchCard.css"
import { useRef } from "react"
import { ZONE_TITLE_IN_MS, ZONE_TITLE_OUT_MS, zoneTitleInFrame, zoneTitleOutFrame, zoneTitleVars } from "../../plan/motion"
import { useZoneMoment } from "../../use-zone-moment"
import { ZoneTitleCard } from "../molecules/ZoneTitleCard"

/**
 * The launch title card (REFINEMENT.md 3, M3): from the moment PLAY or
 * CONTINUE is chosen until Korri lets go of the game, the card covers the
 * zone select. Its bands sweep in from the top right, the name from the
 * left, the ZONE bar from the right, the act disc in four steps; when
 * Korri lets go (`leaving`) everything leaves toward the bottom left in
 * 400 ms, and the binding removes it.
 *
 * The act is the session about to start; ZONE is the surface's own name.
 * The card is the status layer's stage (ZoneSurface passes it): Korri's
 * busy strip stands at its foot, unveiled, and a question stands over it.
 *
 * It takes no focus: what is under it is inert while it shows.
 */
export function ZoneLaunchCard({
  title,
  act,
  system,
  leaving = false,
}: {
  readonly title: string
  readonly act: number
  readonly system?: string
  readonly leaving?: boolean
}) {
  const box = useRef<HTMLDivElement>(null)
  useZoneMoment(box, {
    key: `${title}:${act}:${leaving ? "out" : "in"}`,
    duration: leaving ? ZONE_TITLE_OUT_MS : ZONE_TITLE_IN_MS,
    frame: (t, size) => zoneTitleVars(leaving
      ? zoneTitleOutFrame(t, size.wArt, size.hArt, size.drawing.motion)
      : zoneTitleInFrame(t, size.wArt, size.hArt, size.drawing.motion)),
  })
  return (
    <div className="zone-launch-card" data-leaving={leaving || undefined} ref={box}>
      <ZoneTitleCard act={act} system={system} title={title} />
    </div>
  )
}
