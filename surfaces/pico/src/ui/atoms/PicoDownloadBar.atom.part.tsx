import { PicoDownloadBar } from "./PicoDownloadBar"

export const name = "Download Bar"
export const note = "A download that goes on in the background: Korri's bytes in whole sixteenths, or a walking band with no known end"

export default function PicoDownloadBarPart() {
  return <PicoDownloadBar label="DOWNLOADING 41%" progress={{ done: 344_400_000, total: 840_000_000 }} />
}

export function JustStarted() {
  return <PicoDownloadBar label="DOWNLOADING 0%" progress={{ done: 0, total: 840_000_000 }} />
}

export function AlmostDone() {
  return <PicoDownloadBar label="DOWNLOADING 99%" progress={{ done: 839_000_000, total: 840_000_000 }} />
}

/** Unpacking, or a size Korri does not know: no fraction is claimed. */
export function NoKnownEnd() {
  return <PicoDownloadBar label="DOWNLOADING" progress={{ done: 0 }} />
}
