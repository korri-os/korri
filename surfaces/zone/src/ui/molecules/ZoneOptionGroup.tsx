import "./ZoneOptionGroup.css"
import type { SurfaceSettingItem, SurfaceSettingsStatus } from "@contracts/surface/korri-surface"
import { type ReactNode, useRef } from "react"
import { useZoneFit } from "../../use-zone-fit"
import { useZoneStairClip } from "../../use-zone-stair-clip"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZoneOptionRow } from "./ZoneOptionRow"

/** A row as the group draws it: Korri's item, and what ZoneOptionRow adds to one. */
export interface ZoneOptionGroupRow extends SurfaceSettingItem {
  /** Drawn in place of the value's text (a ZoneSignal, bars); the text still names it. */
  readonly valueNode?: ReactNode
  /** The steel face and inert to A; the description says why. */
  readonly disabled?: boolean
  /** The label is a fact Korri publishes: keep its case. */
  readonly verbatimLabel?: boolean
  /** The value is a fact Korri publishes: keep its case. */
  readonly verbatimValue?: boolean
  /** The description is Zone's own word (CONNECTING), set in caps. */
  readonly descriptionCaps?: boolean
}

/**
 * A titled run of option rows, as Mania heads VIDEO or CONTROLS: the title
 * in caps-2 on a black panel flush with the group's left edge, leaning at
 * its right end, and the rows under it. Where the group's box scrolls (the
 * OPTIONS band), the title holds at the top while its rows pass under it,
 * so the black wedge at the band's left always names the group the cursor
 * is in. The title steps to caps-1 when caps-2 does not fit.
 *
 * A holder that centres the rows in a measure sets
 * `--zone-option-group-rows` (their width); the title's words then start
 * where the rows' words do.
 *
 * The group takes the height it is given. Its rows scroll inside it when
 * they do not fit, and a cut-off row at the bottom edge says there is more;
 * the focused row scrolls itself into view. Korri omits a group it has
 * nothing to say about, so an empty one is a title and nothing else.
 *
 * `status` is Korri's settings status, verbatim: the saving row blinks, the
 * problem row carries the message.
 */
export function ZoneOptionGroup({
  title,
  rows,
  focusedId,
  status = { _tag: "Idle" },
  onPress,
  onStep,
  onFocus,
}: {
  readonly title: string
  readonly rows: readonly ZoneOptionGroupRow[]
  readonly focusedId?: string
  readonly status?: SurfaceSettingsStatus
  readonly onPress?: (settingId: string) => void
  readonly onStep?: (settingId: string, direction: -1 | 1) => void
  readonly onFocus?: (settingId: string) => void
}) {
  const section = useRef<HTMLElement>(null)
  const heading = useRef<HTMLHeadingElement>(null)
  useZoneFit(section, [heading], title)
  useZoneStairClip(heading, { left: "straight", right: "slash" })
  return (
    <section aria-label={title} className="zone-option-group" ref={section}>
      <h2 className="zone-option-group-title" ref={heading}>
        <ZoneCaps>{title}</ZoneCaps>
      </h2>
      <div className="zone-option-group-rows">
        {rows.map((row) => (
          <ZoneOptionRow
            description={row.description}
            descriptionCaps={row.descriptionCaps}
            disabled={row.disabled}
            focused={row.id === focusedId}
            interaction={row.interaction}
            key={row.id}
            label={row.label}
            onFocus={onFocus === undefined ? undefined : () => onFocus(row.id)}
            onPress={onPress === undefined ? undefined : () => onPress(row.id)}
            onStep={onStep === undefined ? undefined : (direction) => onStep(row.id, direction)}
            problem={status._tag === "Problem" && status.settingId === row.id ? status.message : undefined}
            saving={status._tag === "Saving" && status.settingId === row.id}
            value={row.value}
            valueNode={row.valueNode}
            verbatimLabel={row.verbatimLabel}
            verbatimValue={row.verbatimValue}
          />
        ))}
      </div>
    </section>
  )
}
