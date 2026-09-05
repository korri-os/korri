import { PicoTabs } from "./PicoTabs"

export const name = "Tabs"
export const note = "Vertical categories, one lit; the settings sidebar"

export default function PicoTabsPart() {
  return <PicoTabs current={1} onSelect={() => undefined} tabs={["DEVICE", "PLUGINS", "PERMISSIONS"]} />
}
