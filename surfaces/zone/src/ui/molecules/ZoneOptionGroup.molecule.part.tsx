import { fixtureManyRows, fixtureVideoGroup } from "../../fixtures/fixture-dialogs"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneOptionGroup } from "./ZoneOptionGroup"

export const name = "Option group"
export const note = "A group title on a black parallelogram and its rows; the rows scroll under the title"

export const composition = {
  default: [
    { part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "ChoiceFocused" },
    { part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "default" },
  ],
  TwelveRows: [
    { part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "Text" },
    { part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "SensitiveText" },
    { part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "Action" },
    { part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "DestructiveAction" },
  ],
} as const

/** Mania's VIDEO group: five rows, the first focused. */
export default function FiveRows() {
  return <ZoneOptionGroup focusedId={fixtureVideoGroup.items[0]?.id} rows={fixtureVideoGroup.items} title={fixtureVideoGroup.title} />
}

/** One row. */
export function OneRow() {
  return <ZoneOptionGroup focusedId={fixtureManyRows[6]?.id} rows={fixtureManyRows.slice(6, 7)} title="Device" />
}

/** Twelve rows, the ninth focused: the list scrolls to it and a cut-off row shows more. */
export function TwelveRows() {
  return <ZoneOptionGroup focusedId={fixtureManyRows[8]?.id} rows={fixtureManyRows} title="Options" />
}

/** Twelve rows in a strip: the title stays, the rows scroll under it. */
export function TwelveRowsShort() {
  return (
    <ZonePartFrame shape="wide-short">
      <ZoneOptionGroup focusedId={fixtureManyRows[2]?.id} rows={fixtureManyRows} title="Options" />
    </ZonePartFrame>
  )
}

/** Korri published a group with nothing in it: the title alone, no invented row. */
export function Empty() {
  return <ZoneOptionGroup rows={[]} title="System" />
}
