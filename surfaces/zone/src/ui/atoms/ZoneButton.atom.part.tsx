import { ZoneButton } from "./ZoneButton"

export const name = "Button"
export const note = "A black parallelogram verb: lit wears the underline, focused adds the triangles, pressed sinks"

const press = () => undefined

export default function Default() {
  return <ZoneButton label="Play here" onPress={press} />
}

/** The row's default action: the pink underline. */
export function Lit() {
  return <ZoneButton label="Play here" lit onPress={press} />
}

/** The cursor on it: underline and the two red triangles. */
export function Focused() {
  return <ZoneButton focused label="Play here" lit onPress={press} />
}

/** Mid-press: the panel sinks into its underline. */
export function Pressed() {
  return <ZoneButton focused label="Play here" lit onPress={press} pressed />
}

/** Cannot be pressed, and says why in Korri's words. */
export function Disabled() {
  return <ZoneButton disabled label="Play on zao" onPress={press} reason="zao is offline" />
}

/** Disabled with the cursor on it: still reachable, still explained. */
export function DisabledFocused() {
  return <ZoneButton disabled focused label="Play on zao" onPress={press} reason="zao is offline" />
}

export function LongLabel() {
  return <ZoneButton focused label="Continue on hummingbird-lake-east" lit onPress={press} />
}
