import { fixtureRunnerWarnings } from "../../fixtures/fixture-options"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneWarningLines } from "./ZoneWarningLines"

export const name = "Warning lines"
export const note = "Korri's warnings verbatim, red on black slabs, wrapped and never cut"

const lines = fixtureRunnerWarnings._tag === "Closed" ? [] : [
  ...fixtureRunnerWarnings.warnings,
  ...fixtureRunnerWarnings.routes.flatMap(route => route.warnings),
]

/** Two of Korri's warnings. */
export default function Two() {
  return <ZoneWarningLines lines={lines} />
}

/** One short warning: a short slab. */
export function One() {
  return <ZoneWarningLines lines={["Shader preset not found"]} />
}

/** The same warning twice is said once. */
export function Repeated() {
  return <ZoneWarningLines lines={[...lines, ...lines]} />
}

/** A narrow column: each line wraps inside its slab. */
export function Narrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneWarningLines lines={lines} />
    </ZonePartFrame>
  )
}
