import { fixtureProblem } from "../../fixtures/fixture-dialogs"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneDialog } from "./ZoneDialog"

export const name = "Dialog"
export const note = "The blue panel cut by two black wedges; Korri's kicker and reason; NO / YES"

const problem = fixtureProblem._tag === "Problem" ? fixtureProblem : undefined
const headline = problem?.kicker ?? ""
const reason = problem?.reason ?? ""
const noYes = [{ label: "No" }, { label: "Yes" }]

/** Korri's problem with the way back and the retry; NO is lit. */
export default function Default() {
  return <ZoneDialog actions={noYes} headline={headline} lines={[reason]} />
}

/** A long reason: the body scrolls between the headline and the buttons. */
export function LongBody() {
  return (
    <ZonePartFrame shape="wide-short">
      <ZoneDialog
        actions={noYes}
        headline={headline}
        lines={[
          reason,
          "The last three attempts timed out after thirty seconds each.",
          "zao was last seen on this network four minutes ago.",
          "Starting again asks zao to wake first, then to open the game.",
          "Nothing on this device has changed.",
        ]}
      />
    </ZonePartFrame>
  )
}

/** One way out: DISMISS alone when Korri cannot retry. */
export function OneButton() {
  return <ZoneDialog actions={[{ label: "Dismiss" }]} headline={headline} lines={[reason]} />
}

/** The cursor on NO. */
export function FocusedNo() {
  return <ZoneDialog actions={noYes} focused={0} headline={headline} lines={[reason]} />
}

/** The cursor on YES. */
export function FocusedYes() {
  return <ZoneDialog actions={noYes} focused={1} headline={headline} lines={[reason]} />
}

/** Korri names the game: a third line in yellow. */
export function WithTitle() {
  return <ZoneDialog actions={noYes} focused={1} headline={headline} lines={[reason]} title={problem?.gameTitle} />
}
