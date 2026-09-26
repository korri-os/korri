import { useState } from "react"
import type { PicoSettingRowView, PicoSettingsView } from "../../pico-settings-view"
import { PicoRow } from "../atoms/PicoRow"
import { PicoSettingRow } from "../molecules/PicoSettingRow"
import { PicoPanelScreen } from "../templates/PicoPanelScreen"

/**
 * Korri's settings groups as spines, each group's items as rows in its card.
 *
 * Owns which group is showing, because that is a fact about the screen and not
 * about the device. Owns nothing about values: a press hands the row to the
 * surface, which asks Korri, and Korri republishes.
 */
export function PicoSettingsPanel({
  settings,
  onActivate,
  onDismissProblem,
}: {
  readonly settings: PicoSettingsView
  readonly onActivate: (row: PicoSettingRowView) => void
  readonly onDismissProblem: () => void
}) {
  const [current, setCurrent] = useState(0)
  const group = settings.groups[Math.min(current, settings.groups.length - 1)]

  if (group === undefined) {
    return (
      <div className="pico-settings-panel-empty">
        <span className="pico-settings-panel-empty-kicker">NOTHING TO SET</span>
        <span>Korri has no facts or settings to state for this device.</span>
      </div>
    )
  }

  const problem = group.rows.find((row) => typeof row.state === "object")

  return (
    <PicoPanelScreen
      current={current}
      onSelect={setCurrent}
      tabs={settings.groups.map((candidate) => candidate.title)}
      title={group.title}
    >
      <ul className="pico-settings-panel-rows">
        {group.rows.map((row) => (
          <PicoSettingRow key={row.id} onActivate={() => onActivate(row)} row={row} />
        ))}
      </ul>
      {problem === undefined ? null : (
        <div className="pico-settings-panel-clear">
          <PicoRow label="OK" onPress={onDismissProblem} />
        </div>
      )}
    </PicoPanelScreen>
  )
}
