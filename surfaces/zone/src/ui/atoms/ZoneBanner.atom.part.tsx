import { ZoneBanner } from "./ZoneBanner"

export const name = "Banner"
export const note = "The yellow-on-blue hexagon title, used once on a screen"

export default function Default() {
  return <ZoneBanner children="Games" />
}

export function Short() {
  return <ZoneBanner children="OK" />
}

/** A long title: the banner grows; inside a strip it truncates at the box. */
export function Long() {
  return <ZoneBanner children="Save data management" />
}
