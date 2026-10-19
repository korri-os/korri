import { ZoneTabStrip } from "./ZoneTabStrip"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"

export const name = "Tab strip"
export const note = "GAMES, OPTIONS, SAVE DATA shoulder to shoulder; the current one is the banner"

const tabs = [
  { id: "games", label: "Games" },
  { id: "options", label: "Options", sub: "Settings + Wi-Fi" },
  { id: "savedata", label: "Save data", sub: "Identity" },
]

const select = () => undefined

export default function Default() {
  return <ZoneTabStrip active="games" onSelect={select} tabs={tabs} />
}

export function GamesActive() {
  return <ZoneTabStrip active="games" onSelect={select} tabs={tabs} />
}

export function OptionsActive() {
  return <ZoneTabStrip active="options" onSelect={select} tabs={tabs} />
}

export function SavedataActive() {
  return <ZoneTabStrip active="savedata" onSelect={select} tabs={tabs} />
}

/** The cursor on OPTIONS while GAMES is current: lift and underline on OPTIONS. */
export function FocusedOnTab() {
  return <ZoneTabStrip active="games" focused="options" onSelect={select} tabs={tabs} />
}

/** The cursor on the current tab: the banner itself lifts and takes the underline. */
export function FocusedOnBanner() {
  return <ZoneTabStrip active="games" focused="games" onSelect={select} tabs={tabs} />
}

/** A strip of two with no sub-lines. */
export function ShortStrip() {
  return (
    <ZoneTabStrip
      active="games"
      onSelect={select}
      tabs={[
        { id: "games", label: "Games" },
        { id: "options", label: "Options" },
      ]}
    />
  )
}

/** Three long labels in a column a third of the screen wide: the gap gives, then the labels truncate. */
export function ThreeLongLabels() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneTabStrip
        active="achievements"
        focused="configuration"
        onSelect={select}
        tabs={[
          { id: "achievements", label: "Achievements" },
          { id: "configuration", label: "Configuration", sub: "Controllers + display + wi-fi" },
          { id: "management", label: "Save data management", sub: "Identity + backups" },
        ]}
      />
    </ZonePartFrame>
  )
}
