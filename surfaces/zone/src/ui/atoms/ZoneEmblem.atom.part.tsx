import { ZoneEmblem } from "./ZoneEmblem"

export const name = "Emblem"
export const note = "Zone's own 16x16 pixel emblems: key, restore, link, retired key, pad"

/** The key: this device's key (BACK UP, START). */
export default function Key() {
  return <ZoneEmblem name="key" />
}

/** An arrow going into a card: a key from a backup. */
export function Restore() {
  return <ZoneEmblem name="restore" />
}

/** Two chain links: a bunker. */
export function Link() {
  return <ZoneEmblem name="link" />
}

/** The key in greys: a retired key. */
export function Retired() {
  return <ZoneEmblem name="retired" />
}

/** A controller: a seat at first boot. */
export function Pad() {
  return <ZoneEmblem name="pad" />
}
