import "./ZoneTitleCard.css"
import { useRef } from "react"
import { zoneStairCircle } from "../../plan/stair-clip"
import { ZONE_TITLE_BANDS, ZONE_TITLE_DISC, zoneTitleBandClip } from "../../plan/title-card"
import { useZoneWordsFit } from "../../use-zone-words-fit"
import { useZoneMoment, type ZoneMomentBox } from "../../use-zone-moment"
import { useZoneStairClip } from "../../use-zone-stair-clip"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZoneTag } from "../atoms/ZoneTag"
import { ZoneWords } from "../atoms/ZoneWords"

/** The card's drawing that depends on its size alone: the bands' stairs and the disc. */
const layout = (box: ZoneMomentBox) => ({
  ...Object.fromEntries(ZONE_TITLE_BANDS.map((band, index) =>
    [`--zone-tc-band-${index}-shape`, zoneTitleBandClip(band, box.wArt, box.hArt, box.drawing.edges)])),
  "--zone-tc-disc-shape": zoneStairCircle(ZONE_TITLE_DISC, box.drawing.edges),
})

/**
 * The title card, in the register of Mania's zone cards and drawn here:
 * a flat yellow ground, four bands falling at 45 degrees with pixel stairs,
 * the game's title in white caps-3 rising 3 art px over its black panel,
 * ZONE in black caps-2 on the white bar under it, the game's system on a
 * blue tag under the bar, and the orange act disc beside them with ACT on
 * a black tab and the act number in caps-3 (caps-2 from three digits).
 *
 * The act is the session about to start (Korri's playCount + 1); without
 * one there is no disc. ZONE is the surface's own name, never a stage's.
 *
 * The title steps caps-3 to caps-2 to caps-1 to fit, then wraps between
 * words; a word is never cut. On a card taller than wide the disc stands
 * under the words (`ZONE_TITLE_STACK_ASPECT`).
 *
 * A still: every element stands at rest. A moment (the launch, the idle
 * attract) moves the parts through the custom properties it writes on a
 * box round the card: `--zone-tc-ground-x/y`, `--zone-tc-band-N-x/y`,
 * `--zone-tc-name-x`, `--zone-tc-bar-x` (art px) and `--zone-tc-disc`
 * (the disc's scale).
 */
export function ZoneTitleCard({
  title,
  act,
  system,
}: {
  readonly title: string
  /** The act number: the session about to start. */
  readonly act?: number
  /** Korri's subtitle for the game, on the tag under the bar. */
  readonly system?: string
}) {
  const card = useRef<HTMLDivElement>(null)
  const heading = useRef<HTMLHeadingElement>(null)
  const slab = useRef<HTMLSpanElement>(null)
  const bar = useRef<HTMLSpanElement>(null)
  useZoneWordsFit(card, heading, title)
  useZoneStairClip(slab, { left: "slash", right: "slash" })
  useZoneStairClip(bar, { left: "slash", right: "straight" })
  useZoneMoment(card, { key: "layout", duration: 0, frame: () => ({}), layout })
  const digits = act === undefined ? 0 : String(act).length
  return (
    <div className="zone-title-card" data-ground="titlecard" ref={card}>
      <span aria-hidden="true" className="zone-title-card-ground" />
      {ZONE_TITLE_BANDS.map((band, index) => (
        <span aria-hidden="true" className="zone-title-card-band" data-band={index} data-colour={band.colour} key={band.colour} />
      ))}
      <div className="zone-title-card-face" data-act={act === undefined ? "none" : "disc"}>
        <div className="zone-title-card-name">
          <span aria-hidden="true" className="zone-title-card-slab" ref={slab} />
          <h1 className="zone-title-card-title" ref={heading}>
            <ZoneWords>{title}</ZoneWords>
          </h1>
        </div>
        <div className="zone-title-card-zone">
          <span className="zone-title-card-bar" ref={bar}>
            <ZoneCaps ink="ink">Zone</ZoneCaps>
          </span>
          {system === undefined ? null : (
            <span className="zone-title-card-system">
              <ZoneTag kind="system" label={system} />
            </span>
          )}
        </div>
        {act === undefined ? null : (
          <div aria-label={`Act ${act}`} className="zone-title-card-disc" data-digits={digits > 2 ? "3" : "2"} role="img">
            <span aria-hidden="true" className="zone-title-card-act-tab">
              <ZoneCaps>Act</ZoneCaps>
            </span>
            <span aria-hidden="true" className="zone-title-card-act">
              <ZoneCaps>{String(act)}</ZoneCaps>
            </span>
          </div>
        )}
      </div>
    </div>
  )
}
