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
 * A layered value (a launch setting) says under it where the value in force
 * comes from, in Korri's words, and carries DEVICE-ONLY when Korri says it
 * stays on this device.
 *
 * A row that opens one of Korri's groups (RUNNER, FROM FILES) carries the
 * group's caption above it, so rows the player can change and rows only a
 * file sets are told apart.
 *
 * A fact whose value is longer than PICO_FACT_INLINE_MAX characters (a
 * command a hook runs, a folder) stands whole on its own line under the
 * label, wrapping anywhere, rather than squeezing the label beside it.
 *
 * A fact row is not a disabled button. Disabled reads as "you cannot", and the
 * truth is "there is nothing to do" — the software version is a fact, not a
 * locked control. The Wi-Fi network this device is on is a fact too.
 */
/** A fact longer than this many characters moves under its label. */
export const PICO_FACT_INLINE_MAX = 20

export function PicoSettingRow({
  row,
  onActivate,
}: {
  readonly row: PicoSettingRowView
  readonly onActivate: () => void
}) {
  const interactive = row.control.kind !== "fact"
    && !(row.control.kind === "network" && row.control.network.connected)
  const longFact = row.control.kind === "fact" && row.value !== undefined && row.value.length > PICO_FACT_INLINE_MAX
  const failure = typeof row.state !== "object" ? undefined : "problem" in row.state ? row.state.problem : row.state.failed
  const detail = (
    <>
      {row.state === "saving" ? <PicoBadge text="SAVING" tone="info" /> : null}
      {row.control.kind === "network" ? (
        <PicoNetworkMarks network={row.control.network} />
      ) : row.control.kind === "cycle" ? (
        <PicoSegments current={row.control.current} options={row.control.options} />
      ) : row.control.kind === "step" ? (
        <span>{`◀ ${row.control.current} ▶`}</span>
      ) : row.value === undefined || longFact ? null : (
        <span>{row.value}</span>
      )}
      {row.control.kind === "action" || row.control.kind === "text" || row.control.kind === "pick" ? <span aria-hidden>▶</span> : null}
    </>
  )
  return (
    <li className="pico-setting-row">
      {row.section === undefined ? null : <p className="pico-setting-row-section">{row.section}</p>}
      <PicoRow
        danger={row.control.kind === "action" && row.control.destructive}
        detail={detail}
        label={row.label}
        focusable={!interactive}
        onPress={interactive ? onActivate : undefined}
      />
      {longFact ? <p className="pico-setting-row-fact">{row.value}</p> : null}
      {row.source === undefined && row.deviceOnly !== true ? null : (
        <p className="pico-setting-row-source">
          {row.source === undefined ? null : <span>{row.source}</span>}
          {row.deviceOnly === true ? <PicoBadge text="DEVICE-ONLY" /> : null}
        </p>
      )}
      {row.description === undefined ? null : (
        <p className="pico-setting-row-description">{row.description}</p>
      )}
      {failure === undefined ? null : <p className="pico-setting-row-problem">{failure}</p>}
    </li>
  )
}
