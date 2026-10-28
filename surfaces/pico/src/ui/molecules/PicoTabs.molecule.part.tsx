import { useState } from "react"
import { PicoTabs } from "./PicoTabs"

export const name = "Tabs"
export const note = "Vertical categories, one lit; the settings sidebar"

export default function PicoTabsPart() {
  return <PicoTabs current={1} onSelect={() => undefined} tabs={["DEVICE", "PLUGINS", "PERMISSIONS"]} />
}

export function Selectable() {
  const [current, setCurrent] = useState(0)
  return <PicoTabs current={current} onSelect={setCurrent} tabs={["DEVICE", "PLUGINS", "PERMISSIONS"]} />
}

/* More sections than fit, some named by long addresses, one with no break
 * point at all, and two with the same name. */
const MANY = [
  "DEVICE",
  "PLUGINS",
  "OFFICIAL PLUGINS",
  "GIT.EXAMPLE/ADA/RETRO-PLUGINS",
  "FORGE.EXAMPLE/PIXEL-COLLECTIVE/KORRI-HANDHELD-PLUGINS-FOR-SMALL-SCREENS",
  "AVERYVERYVERYLONGREPOSITORYNAMEWITHOUTANYBREAKPOINTS",
  "LAB.EXAMPLE/MIRA/KORRI",
  "LAB.EXAMPLE/MIRA/KORRI",
  "INSTALLED FROM REMOVED REPOSITORIES",
  "OTHER INSTALLED PLUGINS",
  "GAMES",
  "SYSTEM INFORMATION",
]

export function ManyLongSections() {
  return <PicoTabs current={4} onSelect={() => undefined} tabs={MANY} />
}
