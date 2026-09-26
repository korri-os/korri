import { PicoPanelScreen } from "./PicoPanelScreen"

export const name = "Panel Screen"
export const note = "Sections as spines beside a card of the same plastic"

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
