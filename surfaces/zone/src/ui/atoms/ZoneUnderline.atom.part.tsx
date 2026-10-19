import { ZoneCaps } from "./ZoneCaps"
import { ZonePanel } from "./ZonePanel"
import { ZoneUnderline } from "./ZoneUnderline"

export const name = "Underline"
export const note = "The pink lit mark, one drop edge down and left behind a panel"

/** Alone it is the shape at its box's size: the frame, offset. */
export default function Default() {
  return <ZoneUnderline />
}

/** Under a wide panel: a long bar and the sliver along the leading slant. */
export function Wide() {
  return (
    <ZonePanel
      children={
        <>
          <ZoneCaps size="button">Continue on hummingbird-lake</ZoneCaps>
          <ZoneUnderline />
        </>
      }
      edge={false}
    />
  )
}

/** Under a narrow panel: the bar is barely longer than the slant. */
export function Narrow() {
  return (
    <ZonePanel
      children={
        <>
          <ZoneCaps size="button">OK</ZoneCaps>
          <ZoneUnderline />
        </>
      }
      edge={false}
    />
  )
}
