import type { SurfaceNetworkConnection } from "@contracts/surface/korri-surface"
import { ZoneShell } from "./ZoneShell"
import { ZoneKeycap } from "../atoms/ZoneKeycap"
import { ZoneFoot } from "../molecules/ZoneFoot"
import { ZoneTabStrip } from "../molecules/ZoneTabStrip"

export const name = "Shell"
export const note = "Keycap hints in the corners, tabs between, a body, a foot strip"

/* The shell supplies no words of its own: the default state fills each slot
 * with the real control that will stand in it, and the other states keep
 * plain placeholders so the slots themselves stay visible. */
const hintBack = <ZoneKeycap button="B" word="Back" />
const hintConfirm = <ZoneKeycap button="A" word="Confirm" />
const tabItems = [
  { id: "games", label: "Games" },
  { id: "options", label: "Options", sub: "Settings + Wi-Fi" },
  { id: "savedata", label: "Save data", sub: "Identity" },
]
const select = () => undefined
const tabs = <ZoneTabStrip active="games" onSelect={select} tabs={tabItems} />
const wifi: SurfaceNetworkConnection = { _tag: "Wifi", ssid: "hummingbird-lake", strengthPercent: 72 }
const foot = <ZoneFoot build="korri 0.4.1 · 9a1fb22" clock="21:47" connection={wifi} device="Mini" />

const plainTabs = (
  <>
    <span>Games</span>
    <span>Options</span>
    <span>Save data</span>
  </>
)
const plainFoot = (
  <>
    <span>Time 21:47</span>
    <span>Mini</span>
  </>
)
const plainFootEnd = (
  <>
    <span>hummingbird-lake 72%</span>
    <span>korri 0.4.1</span>
  </>
)
const body = <span>Body</span>

export default function Default() {
  return (
    <ZoneShell foot={foot} hintBack={hintBack} hintConfirm={hintConfirm} tabs={tabs}>
      {body}
    </ZoneShell>
  )
}

/** No foot strip: the body takes its height. */
export function NoFoot() {
  return (
    <ZoneShell hintBack={<span>B Back</span>} hintConfirm={<span>A Confirm</span>} tabs={plainTabs}>
      {body}
    </ZoneShell>
  )
}

/** The plan's short strips and narrow type, as a 640 by 480 screen gets. */
export function Short() {
  return (
    <ZoneShell
      band="narrow"
      foot={plainFoot}
      footEnd={plainFootEnd}
      hintBack={<span>B Back</span>}
      hintConfirm={<span>A Confirm</span>}
      strips="short"
      tabs={plainTabs}
    >
      {body}
    </ZoneShell>
  )
}
