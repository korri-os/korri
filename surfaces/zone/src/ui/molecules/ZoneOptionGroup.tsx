import "./ZoneOptionGroup.css"
import type { SurfaceSettingItem, SurfaceSettingsStatus } from "@contracts/surface/korri-surface"
import { ZoneOptionRow } from "./ZoneOptionRow"

/**
 * A titled run of option rows, as Mania heads VIDEO or CONTROLS: the title
 * in caps on a black parallelogram, the rows under it.
 *
 * The group takes the height it is given. Its rows scroll inside it when
 * they do not fit, and a cut-off row at the bottom edge says there is more;
 * the focused row scrolls itself into view. Korri omits a group it has
 * nothing to say about, so an empty one is a title and nothing else.
 *
 * `status` is Korri's settings status, verbatim: the saving row blinks, the
 * problem row carries the message.
 *
 * The title's parallelogram is drawn here; it becomes ZonePanel after the
 * merge.
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
  readonly rows: readonly SurfaceSettingItem[]
  readonly focusedId?: string
  readonly status?: SurfaceSettingsStatus
  readonly onPress?: (settingId: string) => void
  readonly onStep?: (settingId: string, direction: -1 | 1) => void
  readonly onFocus?: (settingId: string) => void
}) {
  return (
    <section aria-label={title} className="zone-option-group">
      <h2 className="zone-option-group-title">{title}</h2>
      <div className="zone-option-group-rows">
        {rows.map((row) => (
          <ZoneOptionRow
            description={row.description}
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
          />
        ))}
      </div>
    </section>
  )
}
