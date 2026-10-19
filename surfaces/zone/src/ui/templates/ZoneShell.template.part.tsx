import { ZoneShell } from "./ZoneShell"

export const name = "Shell"
export const note = "Keycap hints in the corners, tabs between, a body, a foot strip"

/* Plain placeholder children: the shell supplies no words of its own, so a
 * part shows each slot with the kind of text that will stand in it. */
const hintBack = <span>B Back</span>
const hintConfirm = <span>A Confirm</span>
const tabs = (
  <>
    <span>Games</span>
    <span>Options</span>
    <span>Save data</span>
  </>
)
const foot = (
  <>
    <span>Time 21:47</span>
    <span>Mini</span>
  </>
)
const footEnd = (
  <>
    <span>hummingbird-lake 72%</span>
    <span>korri 0.4.1</span>
  </>
)
const body = <span>Body</span>

export default function Default() {
  return (
    <ZoneShell foot={foot} footEnd={footEnd} hintBack={hintBack} hintConfirm={hintConfirm} tabs={tabs}>
      {body}
    </ZoneShell>
  )
}

/** No foot strip: the body takes its height. */
export function NoFoot() {
  return (
    <ZoneShell hintBack={hintBack} hintConfirm={hintConfirm} tabs={tabs}>
      {body}
    </ZoneShell>
  )
}

/** The plan's short strips and narrow type, as a 640 by 480 screen gets. */
export function Short() {
  return (
    <ZoneShell band="narrow" foot={foot} footEnd={footEnd} hintBack={hintBack} hintConfirm={hintConfirm} strips="short" tabs={tabs}>
      {body}
    </ZoneShell>
  )
}
