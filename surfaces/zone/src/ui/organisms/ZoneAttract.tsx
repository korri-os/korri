import "./ZoneAttract.css"
import { useRef } from "react"
import { ZONE_ATTRACT_MS, zoneAttractFrame, zoneTitleVars } from "../../plan/motion"
import { useZoneMoment } from "../../use-zone-moment"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZoneTitleCard } from "../molecules/ZoneTitleCard"

/**
 * The idle attract (REFINEMENT.md 3, M6), one game's turn of 6000 ms: the
 * game's title card sweeps in (M3), holds, sweeps out, and the game's own
 * cover stands as the "stage", letterboxed
 * on black and panning 32 art px, with the HUD at the top left (TIME is
 * Korri's clock, PLAYS its count) and PRESS ANY BUTTON blinking at the
 * foot. The binding moves to the next game every turn; `turn` names it, so
 * the clock starts again with the next title card.
 *
 * The words are true: any input wakes Zone, the cursor where it was. With
 * no art, the card's stripe pattern fills the stage. The stage stands
 * under the card the whole turn, so a turn's first frames sweep the card
 * in over the game, never over the page. Under reduced motion the title
 * card stands still and nothing blinks.
 */
export function ZoneAttract({
  turn,
  title,
  act,
  system,
  art,
  clock,
  plays,
}: {
  /** Names this turn: a new value starts the turn again. */
  readonly turn: string
  readonly title: string
  readonly act: number
  readonly system?: string
  /** The stage: Korri's cover art. */
  readonly art?: string
  /** Korri's clock, verbatim. Absent: no TIME. */
  readonly clock?: string
  /** Korri's count of sessions. Absent: the dashes. */
  readonly plays?: number
}) {
  const box = useRef<HTMLDivElement>(null)
  useZoneMoment(box, {
    key: turn,
    duration: ZONE_ATTRACT_MS,
    // Reduced motion: the title card at rest.
    rest: 1000,
    frame: (t, size) => {
      const frame = zoneAttractFrame(t, size.wArt, size.hArt, size.drawing.motion)
      return {
        ...zoneTitleVars(frame.card),
        "--zone-attract-card": frame.card === undefined ? "hidden" : "visible",
        "--zone-attract-pan": frame.panX,
        "--zone-attract-prompt": frame.prompt ? "visible" : "hidden",
      }
    },
  })
  return (
    <div className="zone-attract" ref={box}>
      <div aria-hidden="true" className="zone-attract-stage" data-art={art === undefined ? "none" : "art"}>
        <div className="zone-attract-pan">
          {art === undefined ? null : <img alt="" className="zone-attract-art" src={art} />}
        </div>
        <div className="zone-attract-hud">
          {clock === undefined ? null : (
            <span className="zone-attract-hud-line">
              <ZoneCaps ink="hud">Time</ZoneCaps>
              <ZoneCaps fact>{clock}</ZoneCaps>
            </span>
          )}
          <span className="zone-attract-hud-line">
            <ZoneCaps ink="hud">Plays</ZoneCaps>
            {plays === undefined ? <ZoneCaps>--</ZoneCaps> : <ZoneCaps fact>{String(plays)}</ZoneCaps>}
          </span>
        </div>
        <p className="zone-attract-prompt">
          <ZoneCaps>Press any button</ZoneCaps>
        </p>
      </div>
      <div className="zone-attract-card">
        <ZoneTitleCard act={act} system={system} title={title} />
      </div>
    </div>
  )
}
