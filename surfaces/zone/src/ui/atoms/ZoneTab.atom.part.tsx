import { ZoneTab } from "./ZoneTab"

export const name = "Tab"
export const note = "A slanted tab with a sub-line; the current one is the banner; the focused one lifts"

const select = () => undefined

export default function Default() {
  return <ZoneTab label="Options" onSelect={select} sub="Settings + Wi-Fi" />
}

export function NoSub() {
  return <ZoneTab label="Games" onSelect={select} />
}

export function Focused() {
  return <ZoneTab focused label="Save data" onSelect={select} sub="Identity" />
}

/** The current tab: the banner. */
export function Active() {
  return <ZoneTab active label="Games" onSelect={select} />
}

/** The current tab with the cursor on it: the banner lifts and the underline follows its hexagon. */
export function ActiveFocused() {
  return <ZoneTab active focused label="Games" onSelect={select} />
}

export function LongLabel() {
  return <ZoneTab label="Save data management" onSelect={select} sub="Identity, backups and retired keys" />
}
