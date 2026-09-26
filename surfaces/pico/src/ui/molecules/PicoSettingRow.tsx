import type { PicoSettingRowView } from "../../pico-settings-view"
import { PicoBadge } from "../atoms/PicoBadge"
import { PicoRow } from "../atoms/PicoRow"
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
  const detail = (
    <>
      {row.state === "saving" ? <PicoBadge text="SAVING" tone="info" /> : null}
      {row.control.kind === "cycle" ? (
        <PicoSegments current={row.control.current} options={row.control.options} />
      ) : row.value === undefined ? null : (
        <span>{row.value}</span>
      )}
      {row.control.kind === "action" ? <span aria-hidden>▶</span> : null}
    </>
  )
  return (
    <li className="pico-setting-row">
      <PicoRow
        danger={row.control.kind === "action" && row.control.destructive}
        detail={detail}
        label={row.label}
        onPress={interactive ? onActivate : undefined}
      />
      {row.description === undefined ? null : (
        <p className="pico-setting-row-description">{row.description}</p>
      )}
      {typeof row.state === "object" ? (
        <p className="pico-setting-row-problem">{row.state.problem}</p>
      ) : null}
    </li>
  )
}
