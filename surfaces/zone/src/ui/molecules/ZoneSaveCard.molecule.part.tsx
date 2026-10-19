import { fixtureCard } from "../../fixtures/zone-cards"
import { ZoneSaveCard } from "./ZoneSaveCard"

export const name = "Save card"
export const note = "MANIA MODE's vertical card: the striped NEW header, the dark well, one tag"

const celeste = fixtureCard("celeste")

/* A stand-in for the QR code the identity backup will draw: a checked grid
 * of the kind the well will hold. Not a component: a fixture node. */
const qrPlaceholder = (
  <svg aria-label="QR code" role="img" viewBox="0 0 7 7" width="80%" height="80%">
    <rect fill="currentColor" height="7" width="7" />
    <path d="M1 1h2v2H1zM4 1h2v2H4zM1 4h2v2H1zM4 4h1v1H4zM5 5h1v1H5z" fill="var(--zone-panel)" />
  </svg>
)

export default function Default() {
  return <ZoneSaveCard body={{ kind: "art", src: celeste.art! }} header="New" />
}

/** The NEW card: header only, the well empty of a tag. */
export function New() {
  return <ZoneSaveCard body={{ kind: "art", src: celeste.art! }} header="New" />
}

/** A line of text in the well, with the SAVED tag. */
export function WithText() {
  return <ZoneSaveCard body={{ kind: "text", text: "This device" }} header="Back up" tag={{ kind: "saved", label: "Saved" }} />
}

/** A QR placeholder node in the well, with the OPEN tag. */
export function WithQr() {
  return <ZoneSaveCard body={{ kind: "node", node: qrPlaceholder }} header="Back up" tag={{ kind: "open", label: "Open" }} />
}

/** Focused: the pink edge and the triangles above and below the well. */
export function Focused() {
  return <ZoneSaveCard body={{ kind: "art", src: celeste.art! }} focused header="Switch" tag={{ kind: "here", label: "Here" }} />
}

/** A header too long for the band: the caps shrink, then truncate. */
export function LongHeader() {
  return <ZoneSaveCard body={{ kind: "text", text: "Retired" }} header="Retired identity backup" tag={{ kind: "remote", label: "Zao" }} />
}
