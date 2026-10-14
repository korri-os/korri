import "./PicoSettingRow.css"
import type { PicoSettingRowView } from "../../pico-settings-view"
import { PicoBadge } from "../atoms/PicoBadge"
import { PicoRow } from "../atoms/PicoRow"
import { PicoSegments } from "../atoms/PicoSegments"
import { PicoNetworkMarks } from "./PicoNetworkMarks"

/**
 * One setting: its label on the left, its state on the right, as a button when
 * Korri allows an interaction and as plain text when it does not.
 *
 * A fact row is not a disabled button. Disabled reads as "you cannot", and the
 * truth is "there is nothing to do" — the software version is a fact, not a
 * locked control. The Wi-Fi network this device is on is a fact too.
 */
export function PicoSettingRow({
  row,
  onActivate,
}: {
  readonly row: PicoSettingRowView
  readonly onActivate: () => void
}) {
  const interactive = row.control.kind !== "fact"
    && !(row.control.kind === "network" && row.control.network.connected)
  const failure = typeof row.state !== "object" ? undefined : "problem" in row.state ? row.state.problem : row.state.failed
  const detail = (
    <>
      {row.state === "saving" ? <PicoBadge text="SAVING" tone="info" /> : null}
      {row.control.kind === "network" ? (
        <PicoNetworkMarks network={row.control.network} />
      ) : row.control.kind === "cycle" ? (
        <PicoSegments current={row.control.current} options={row.control.options} />
      ) : row.control.kind === "step" ? (
        <span>{`◀ ${row.control.current.toUpperCase()} ▶`}</span>
      ) : row.value === undefined ? null : (
        <span>{row.value}</span>
      )}
      {row.control.kind === "action" || row.control.kind === "text" ? <span aria-hidden>▶</span> : null}
    </>
  )
  return (
    <li className="pico-setting-row">
      <PicoRow
        danger={row.control.kind === "action" && row.control.destructive}
        detail={detail}
        label={row.label}
        focusable={!interactive}
        onPress={interactive ? onActivate : undefined}
      />
      {row.description === undefined ? null : (
        <p className="pico-setting-row-description">{row.description}</p>
      )}
      {failure === undefined ? null : <p className="pico-setting-row-problem">{failure}</p>}
    </li>
  )
}
