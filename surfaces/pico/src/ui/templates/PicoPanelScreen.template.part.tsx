import { fixtureModel } from "../../fixtures/fixture-host"
import { picoSettingsViewFromModel } from "../../pico-settings-view"
import { PicoSettingRow } from "../molecules/PicoSettingRow"
import { PicoPanelScreen } from "./PicoPanelScreen"

export const name = "Panel Screen"
export const note = "Sections as spines beside a card of the same plastic"

export function SecondCategory() {
  const settings = picoSettingsViewFromModel(fixtureModel)
  const group = settings.groups[1]!
  return <PicoPanelScreen current={1} onSelect={() => undefined} tabs={settings.groups.map(group => group.title)} title={group.title}>
    <ul>{group.rows.map(row => <PicoSettingRow key={row.id} row={row} onActivate={() => undefined} />)}</ul>
  </PicoPanelScreen>
}

export function SingleCategory() {
  const group = picoSettingsViewFromModel(fixtureModel).groups[0]!
  return <PicoPanelScreen current={0} onSelect={() => undefined} tabs={[group.title]} title={group.title}>
    <ul>{group.rows.map(row => <PicoSettingRow key={row.id} row={row} onActivate={() => undefined} />)}</ul>
  </PicoPanelScreen>
}

export default function PicoPanelScreenPart() {
  return (
    <PicoPanelScreen
      current={0}
      onSelect={() => undefined}
      tabs={["DEVICE", "PLUGINS"]}
      title="DEVICE"
    >
      <p>Contents of the selected category.</p>
    </PicoPanelScreen>
  )
}
