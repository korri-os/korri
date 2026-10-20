import { fixtureCard } from "../../fixtures/zone-cards"
import { ZoneSaveCard } from "./ZoneSaveCard"

export const name = "Save card"
export const note = "MANIA MODE's save card: the striped header and white bar, the pale window with Zone's emblem and word, the status bar"

const celeste = fixtureCard("celeste")

/* A stand-in for a code the well could hold: a checked grid. Not a
 * component: a fixture node. */
const qrPlaceholder = (
  <svg aria-label="QR code" role="img" viewBox="0 0 7 7" width="80%" height="80%">
    <rect fill="currentColor" height="7" width="7" />
    <path d="M1 1h2v2H1zM4 1h2v2H4zM1 4h2v2H1zM4 4h1v1H4zM5 5h1v1H5z" fill="var(--zone-panel)" />
  </svg>
)

export default function Default() {
  return <ZoneSaveCard body={{ kind: "text", text: "Start", emblem: "key" }} header="New" />
}

/** The NEW card: header only, no status bar; the window holds art. */
export function New() {
  return <ZoneSaveCard body={{ kind: "art", src: celeste.art! }} header="New" />
}

/** BACK UP: the key over THIS KEY, the BACKED UP bar at the foot. */
export function WithText() {
  return <ZoneSaveCard body={{ kind: "text", text: "This device", emblem: "key" }} header="Back up" tag={{ kind: "saved", label: "Saved" }} />
}

/** A node in the well, with the NEW bar. */
export function WithQr() {
  return <ZoneSaveCard body={{ kind: "node", node: qrPlaceholder }} header="Back up" tag={{ kind: "open", label: "Open" }} />
}

/** Focused: the pink edge and the triangles over and under the window. */
export function Focused() {
  return <ZoneSaveCard body={{ kind: "text", text: "From backup", emblem: "restore" }} focused header="Switch" tag={{ kind: "here", label: "Here" }} />
}

/** A header too long for the bar: the caps step down, then truncate; a key breaks after its ellipsis. */
export function LongHeader() {
  return <ZoneSaveCard body={{ kind: "text", text: "npub1qy3…8x7lk2m0", verbatim: true, emblem: "retired" }} header="Retired identity backup" tag={{ kind: "remote", label: "Zao" }} />
}
