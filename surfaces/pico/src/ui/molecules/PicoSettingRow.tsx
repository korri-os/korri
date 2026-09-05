import type { PicoSettingRowView } from "../../pico-settings-view"
import { PicoBadge } from "../atoms/PicoBadge"
import { PicoSegments } from "../atoms/PicoSegments"

/**
 * One setting: its label on the left, its state on the right, as a button when
 * Korri allows an interaction and as plain text when it does not.
 *
 * A fact row is not a disabled button. Disabled reads as "you cannot", and the
 * truth is "there is nothing to do" — the software version is a fact, not a
 * locked control.
 */
export function PicoSettingRow({
  row,
  onActivate,
}: {
  readonly row: PicoSettingRowView
  readonly onActivate: () => void
}) {
  const interactive = row.control.kind !== "fact"
  const Tag = interactive ? "button" : "div"
  return (
    <li className="pico-setting-row-item">
      <Tag
        className="pico-setting-row"
        data-destructive={
          row.control.kind === "action" && row.control.destructive ? "true" : undefined
        }
        {...(interactive ? { onClick: onActivate, type: "button" as const } : {})}
      >
        <span className="pico-setting-row-label">{row.label}</span>
        <span className="pico-setting-row-state">
          {row.state === "saving" ? <PicoBadge text="SAVING" tone="info" /> : null}
          {row.control.kind === "cycle" ? (
            <PicoSegments current={row.control.current} options={row.control.options} />
          ) : row.value === undefined ? null : (
            <span className="pico-setting-row-value">{row.value}</span>
          )}
          {row.control.kind === "action" ? (
            <span aria-hidden className="pico-setting-row-go">▶</span>
          ) : null}
        </span>
      </Tag>
      {row.description === undefined ? null : (
        <p className="pico-setting-row-description">{row.description}</p>
      )}
      {typeof row.state === "object" ? (
        <p className="pico-setting-row-problem">{row.state.problem}</p>
      ) : null}
    </li>
  )
}
