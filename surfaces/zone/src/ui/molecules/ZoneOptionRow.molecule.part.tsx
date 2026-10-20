import {
  fixtureActionRow,
  fixtureChoiceRow,
  fixtureDestructiveRow,
  fixtureEmptyTextRow,
  fixtureLongRow,
  fixtureSecretRow,
  fixtureTextRow,
} from "../../fixtures/fixture-dialogs"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneOptionRow } from "./ZoneOptionRow"

export const name = "Option row"
export const note = "Label and value as two black parallelograms; the interaction decides what the value does"

/** A choice row at rest: no triangles until the cursor lands on it. */
export default function ChoiceUnfocused() {
  return <ZoneOptionRow interaction={fixtureChoiceRow.interaction} label={fixtureChoiceRow.label} value={fixtureChoiceRow.value} />
}

/** The cursor on a choice: red triangles flank the value, the pink underline under the label. */
export function ChoiceFocused() {
  return <ZoneOptionRow focused interaction={fixtureChoiceRow.interaction} label={fixtureChoiceRow.label} value={fixtureChoiceRow.value} />
}

/** A text row: the value as Korri gives it; A raises the keyboard. */
export function Text() {
  return <ZoneOptionRow focused interaction={fixtureTextRow.interaction} label={fixtureTextRow.label} value={fixtureTextRow.value} />
}

/** No value from Korri: its placeholder, dimmed, in the value's place. */
export function TextPlaceholder() {
  return <ZoneOptionRow focused interaction={fixtureEmptyTextRow.interaction} label={fixtureEmptyTextRow.label} value={fixtureEmptyTextRow.value} />
}

/** A secret: Korri's status word as given (never dots, never the secret) and its clear label on the X keycap. */
export function SensitiveText() {
  return <ZoneOptionRow focused interaction={fixtureSecretRow.interaction} label={fixtureSecretRow.label} value={fixtureSecretRow.value} />
}

/** A secret Korri holds none of: its word for that, and no clear key. */
export function SensitiveNotSet() {
  return (
    <ZoneOptionRow
      focused
      interaction={{ kind: "sensitiveText", placeholder: "Paste API key", maxLength: 256 }}
      label={fixtureSecretRow.label}
      value="Not configured"
    />
  )
}

/** An action row: the value is the command. */
export function Action() {
  return <ZoneOptionRow focused interaction={fixtureActionRow.interaction} label={fixtureActionRow.label} value={fixtureActionRow.value} />
}

/** A destructive action: the command in red. */
export function DestructiveAction() {
  return <ZoneOptionRow focused interaction={fixtureDestructiveRow.interaction} label={fixtureDestructiveRow.label} value={fixtureDestructiveRow.value} />
}

/** Korri has not answered yet: the value is a blinking caret. */
export function Saving() {
  return <ZoneOptionRow focused interaction={fixtureChoiceRow.interaction} label={fixtureChoiceRow.label} saving value={fixtureChoiceRow.value} />
}

/** Korri refused: its message in red under the row. */
export function Problem() {
  return (
    <ZoneOptionRow
      focused
      interaction={fixtureTextRow.interaction}
      label={fixtureTextRow.label}
      problem="That name is already taken on this network"
      value={fixtureTextRow.value}
    />
  )
}

/** A long label and a long value in a column: both wrap, nothing is cut. */
export function LongNarrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneOptionRow focused interaction={fixtureLongRow.interaction} label={fixtureLongRow.label} value={fixtureLongRow.value} />
    </ZonePartFrame>
  )
}

/** The focused row shows Korri's description under it. */
export function DescriptionShown() {
  return (
    <ZoneOptionRow
      description={fixtureChoiceRow.description}
      focused
      interaction={fixtureChoiceRow.interaction}
      label={fixtureChoiceRow.label}
      value={fixtureChoiceRow.value}
    />
  )
}
