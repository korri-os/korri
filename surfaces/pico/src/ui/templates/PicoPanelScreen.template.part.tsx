import { PicoPanelScreen } from "./PicoPanelScreen"

export const name = "Panel Screen"
export const note = "Categories beside their contents; every settings-like screen shares it"

export default function PicoPanelScreenPart() {
  return (
    <PicoPanelScreen
      current={0}
      footer="pico-dev"
      onSelect={() => undefined}
      tabs={["DEVICE", "PLUGINS"]}
      title="DEVICE"
    >
      <p>Contents of the selected category.</p>
    </PicoPanelScreen>
  )
}
