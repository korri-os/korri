import "./ZoneTally.css"
import { useRef } from "react"
import {
  ZONE_TALLY_OUT_MS,
  zoneTallyCountMs,
  zoneTallyOutFrame,
  zoneTallyRowFrame,
} from "../../plan/motion"
import { zoneTallyCounts, zoneTallyRows, zoneTallyValue, type ZoneTallyFacts } from "../../plan/tally"
import { useZoneFit } from "../../use-zone-fit"
import { useZoneWordsFit } from "../../use-zone-words-fit"
import { useZoneMoment } from "../../use-zone-moment"
import { useZoneStairClip } from "../../use-zone-stair-clip"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZoneWords } from "../atoms/ZoneWords"
import { ZoneGround } from "./ZoneGround"

/**
 * The results tally after a session (REFINEMENT.md 3, M5), on SPECIAL
 * STAGE's yellow: the game's title on a black panel, then a row for each
 * fact Korri's counts allow, its label in HUD ink and its value counting
 * up on a red bar. TIME is this session's length (when known), PLAYS the
 * new count, TOTAL the new total. Korri's numbers, Zone's words; nothing
 * is invented, and the binding shows a tally only when the count rose by
 * exactly one.
 *
 * Rows enter 300 ms apart and each value counts up in 60 steps over
 * 1200 ms with its bar. `counted` shows the end at once (a press skipped
 * the count-up); `leaving` slides it away. The whole tally is one button:
 * A on it is a press, as the host's buttons are.
 */
export function ZoneTally({
  gameId,
  title,
  seconds,
  plays,
  total,
  counted = false,
  leaving = false,
  focused = false,
  onPress,
  onFocus,
}: ZoneTallyFacts & {
  /** Names this tally: a new one counts again. */
  readonly gameId: string
  readonly title: string
  readonly counted?: boolean
  readonly leaving?: boolean
  readonly focused?: boolean
  readonly onPress?: () => void
  readonly onFocus?: () => void
}) {
  const box = useRef<HTMLDivElement>(null)
  const rowsBox = useRef<HTMLSpanElement>(null)
  const slab = useRef<HTMLSpanElement>(null)
  const heading = useRef<HTMLSpanElement>(null)
  const facts: ZoneTallyFacts = { plays, ...(seconds === undefined ? {} : { seconds }), ...(total === undefined ? {} : { total }) }
  const rows = zoneTallyRows(facts)
  const end = zoneTallyCountMs(rows.length)
  useZoneWordsFit(box, heading, title)
  useZoneFit(box, [rowsBox], rows.map(row => zoneTallyValue(row.kind, facts)).join("|"))
  useZoneStairClip(slab, { left: "slash", right: "slash" })
  useZoneMoment(box, {
    key: `${gameId}:${plays}:${counted ? "end" : "count"}:${leaving ? "out" : "in"}`,
    duration: leaving ? ZONE_TALLY_OUT_MS : counted ? 0 : end,
    frame: (t, size) => {
      const at = counted || leaving ? end : t
      return {
        "--zone-tally-out": leaving ? zoneTallyOutFrame(t, size.hArt) : 0,
        ...Object.assign({}, ...rows.map((row, index) => {
          const frame = zoneTallyRowFrame(at, index)
          return {
            [`--zone-tally-row-${index}-x`]: frame.x,
            [`--zone-tally-row-${index}-shown`]: frame.shown ? 1 : 0,
            [`--zone-tally-row-${index}-bar`]: Number((frame.count * row.share).toFixed(4)),
            ...zoneTallyCounts(row.kind, facts, frame.count),
          }
        })),
      }
    },
  })
  return (
    <div className="zone-tally" data-ground="tally" data-leaving={leaving || undefined} ref={box}>
      <ZoneGround ground="tally" />
      <button
        className="zone-tally-panel"
        data-focused={focused ? "true" : undefined}
        onClick={onPress}
        onFocus={onFocus}
        type="button"
      >
        <span className="zone-tally-head">
          <span aria-hidden="true" className="zone-tally-slab" ref={slab} />
          <span className="zone-tally-title" ref={heading}>
            <ZoneWords>{title}</ZoneWords>
          </span>
        </span>
        <span className="zone-tally-rows" ref={rowsBox}>
          {rows.map((row, index) => (
            <span className="zone-tally-row" data-kind={row.kind} data-row={index} key={row.kind}>
              <span className="zone-tally-label">
                <ZoneCaps ink="hud">{row.label}</ZoneCaps>
              </span>
              <span aria-hidden="true" className="zone-tally-bar" data-row={index} />
              <span
                aria-hidden="true"
                className="zone-tally-count"
                data-hours={row.kind === "total" && (total ?? 0) >= 3600 ? "on" : undefined}
                data-kind={row.kind}
              />
              <span className="zone-tally-value">{zoneTallyValue(row.kind, facts)}</span>
            </span>
          ))}
        </span>
      </button>
    </div>
  )
}
