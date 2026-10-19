import { ZoneActionRow } from "./ZoneActionRow"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"

export const name = "Action row"
export const note = "The verbs under the focused card: one lit, one focused, one disabled with its reason"

const press = () => undefined

const two = [
  { id: "here", label: "Play here" },
  { id: "zao", label: "Play on zao" },
]

const three = [
  { id: "continue", label: "Continue here" },
  { id: "here", label: "Play here" },
  { id: "zao", label: "Play on zao" },
]

export default function Default() {
  return <ZoneActionRow actions={two} lit="here" onPress={press} />
}

export function OneButton() {
  return <ZoneActionRow actions={[{ id: "here", label: "Play" }]} lit="here" onPress={press} />
}

export function TwoButtons() {
  return <ZoneActionRow actions={two} lit="here" onPress={press} />
}

export function ThreeButtons() {
  return <ZoneActionRow actions={three} lit="continue" onPress={press} />
}

export function FocusedFirst() {
  return <ZoneActionRow actions={three} focused="continue" lit="continue" onPress={press} />
}

export function FocusedSecond() {
  return <ZoneActionRow actions={three} focused="here" lit="continue" onPress={press} />
}

export function FocusedThird() {
  return <ZoneActionRow actions={three} focused="zao" lit="continue" onPress={press} />
}

/** One cannot be pressed and says why; the cursor can still reach it. */
export function DisabledOne() {
  return (
    <ZoneActionRow
      actions={[
        { id: "here", label: "Play here" },
        { id: "zao", label: "Play on zao", disabled: true, reason: "zao is offline" },
      ]}
      focused="zao"
      lit="here"
      onPress={press}
    />
  )
}

/** Mid-press on the lit action. */
export function PressedLit() {
  return <ZoneActionRow actions={two} focused="here" lit="here" onPress={press} pressed="here" />
}

/** Long labels in a column a third of the screen wide: the row wraps; nothing is cut. */
export function LongLabelsNarrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneActionRow
        actions={[
          { id: "continue", label: "Continue on hummingbird-lake" },
          { id: "zao", label: "Play on living-room-zao" },
        ]}
        focused="zao"
        lit="continue"
        onPress={press}
      />
    </ZonePartFrame>
  )
}
