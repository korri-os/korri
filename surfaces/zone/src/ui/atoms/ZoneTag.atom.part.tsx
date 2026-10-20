import { ZoneTag } from "./ZoneTag"

export const name = "Tag"
export const note = "A coloured slab with caps: system, HERE, a remote device, SAVED, OPEN, CONTINUE, DOWNLOAD"

export default function Default() {
  return <ZoneTag kind="system" label="PICO-8" />
}

/** The system tag in blue, white caps outlined. */
export function System() {
  return <ZoneTag kind="system" label="Mega Drive" />
}

/** This device: yellow with black ink. */
export function Here() {
  return <ZoneTag kind="here" label="Here" />
}

/** A remote device by name, in red. */
export function Remote() {
  return <ZoneTag kind="remote" label="Zao" />
}

/** A save that is backed up: mint with black ink. */
export function Saved() {
  return <ZoneTag kind="saved" label="Saved" />
}

/** A save that is open: orange with black ink. */
export function Open() {
  return <ZoneTag kind="open" label="Open" />
}

/** The continue mark: white with black ink, as the NEW header. */
export function Continue() {
  return <ZoneTag kind="continue" label="Continue" />
}

/** A game that needs a download before it launches: orange with black ink, the size beside the word. */
export function Download() {
  return <ZoneTag kind="download" label="Download 1.5 GB" />
}

/** A tag cut on its right, as the first of a row. */
export function First() {
  return <ZoneTag join="first" kind="system" label="GBA" />
}

/** A tag cut on both ends, as the middle of a row. */
export function Middle() {
  return <ZoneTag join="middle" kind="here" label="Here" />
}

/** A device name no tag was drawn for: the tag grows with it. */
export function LongText() {
  return <ZoneTag kind="remote" label="hummingbird-lake-living-room" />
}
