import { ZoneCaps } from "./ZoneCaps"

export const name = "Caps"
export const note = "White chunky caps with the black outline and the blue edge, at each size"

export default function Default() {
  return <ZoneCaps size="button" children="Play here" />
}

/** One word with no break point: it truncates with an ellipsis inside its box. */
export function LongWord() {
  return <ZoneCaps size="foot" children="Hummingbird-lake-east-wing-upstairs-office-extender-5ghz" />
}

export function Tab() {
  return <ZoneCaps size="tab" children="Save data" />
}

export function Hint() {
  return <ZoneCaps size="hint" children="Confirm" />
}

export function Button() {
  return <ZoneCaps size="button" children="Play on zao" />
}

export function Foot() {
  return <ZoneCaps size="foot" children="21:47" />
}
