import { useState } from "react"
import { PicoScopeTabs } from "./PicoScopeTabs"

export const name = "Scope Tabs"
export const note = "Which runners a layer's launch settings apply to: any runner, a family, each runner"

const SCOPES = [
  { label: "Any runner", kind: "any" as const },
  { label: "Family retroarch", kind: "family" as const },
  { label: "mGBA", kind: "runner" as const },
  { label: "gpSP", kind: "runner" as const },
]

export default function PicoScopeTabsPart() {
  return (
    <PicoScopeTabs current={2} label="Runner" onSelect={() => undefined} scopes={SCOPES} />
  )
}

export function Selectable() {
  const [current, setCurrent] = useState(0)
  return (
    <PicoScopeTabs current={current} label="Runner" onSelect={setCurrent} scopes={SCOPES} />
  )
}

/** One runner only: the any-runner tab still stands beside it. */
export function OneRunner() {
  return (
    <PicoScopeTabs current={0} label="Runner" onSelect={() => undefined} scopes={[SCOPES[0] ?? { label: "Any runner", kind: "any" }, { label: "Neverball", kind: "runner" }]} />
  )
}

/** Many runners, long names and one with no break point: the tabs wrap. */
export function ManyLong() {
  return (
    <PicoScopeTabs
        current={5}
        label="Runner"
        onSelect={() => undefined}
        scopes={[
          ...SCOPES,
          { label: "Family mednafen", kind: "family" },
          { label: "Mednafen (Beetle GBA, accuracy build)", kind: "runner" },
          { label: "Superextraordinarilyuncompromisingemulator", kind: "runner" },
        ]}
      />
  )
}
