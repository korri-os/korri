import "./PicoSettingsPanel.css"
import type { PicoSettingRowView, PicoSettingsView } from "../pico-settings-view"
import { PicoRow } from "../ui/atoms/PicoRow"
import { PicoSettingRow } from "../ui/molecules/PicoSettingRow"
import { PicoPanelScreen } from "../ui/templates/PicoPanelScreen"

/**
 * Page content: Korri's settings groups fill the shared panel-screen template.
 * PicoSettings adds the screen shell and action handling around this instance.
 *
 * Holds no state. Which group shows is navigation (`Settings.group` in
 * src/state/navigation.ts): the owner passes it in and hears about a new
 * choice through `onGroup`. Owns nothing about values either: a press hands the
 * row to the surface, which asks Korri, and Korri republishes.
 */
export function PicoSettingsPanel({
  settings,
  group,
  onGroup,
  onActivate,
  onDismissProblem,
}: {
  readonly settings: PicoSettingsView
  /** The group showing. A group Korri stopped publishing shows the last one. */
  readonly group: number
  readonly onGroup: (index: number) => void
  readonly onActivate: (row: PicoSettingRowView) => void
  readonly onDismissProblem: () => void
}) {
  const current = Math.min(group, settings.groups.length - 1)
  const shown = settings.groups[current]

  if (shown === undefined) {
    return (
      <div className="pico-settings-panel-empty">
        <span className="pico-settings-panel-empty-kicker">NOTHING TO SET</span>
        <span>Korri has no facts or settings to state for this device.</span>
      </div>
    )
  }

  const problem = shown.rows.find((row) => typeof row.state === "object")

  return (
    <PicoPanelScreen
      current={current}
      onSelect={onGroup}
      tabs={settings.groups.map((candidate) => candidate.title)}
      title={shown.title}
    >
      <ul className="pico-settings-panel-rows">
        {shown.rows.map((row) => (
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
